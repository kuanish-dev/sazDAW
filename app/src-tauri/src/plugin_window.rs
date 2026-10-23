//! Plugin windows: native Tauri windows with the VST3 plugin's own interface (`IPlugView`) embedded.
//!
//! The entity's job is to run the windows: open on request (or raise one already open), attach the plugin view to the window,
//! pass size changes to the plugin, call `PluginEditors::sync` on a timer, and close the window together with the interface.
//! What the plugin shows and where its edits go is `PluginEditors`' business (the contract); here are only the windows.
//!
//! Everything that touches the plugin or the window runs on the MAIN thread (`AppHandle::run_on_main_thread`): both AppKit
//! and plugin interfaces require it. The window is a `tauri::window::WindowBuilder` (no web page): the plugin view sits right in its `NSView`.
//! This part of the app knows about Tauri, so `daw-rpc` does not contain it: the `pluginEditor.open` method is intercepted in the
//! `rpc` command before the shared table (see "Plugin windows" in `crates/daw-vst3/CLAUDE.md`). It is also the app's
//! `PluginStateFlush`: the RPC table asks it before every method that saves or renders the project, whichever door
//! (the window or the AI server) the call came through.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use daw_contracts::codes::{plugins, rpc};
use daw_contracts::{AppLog, ContractError, EditorId, EditorTarget, LogEntry, NativeView, PluginEditors, PluginStateFlush};
use serde_json::Value;
use tauri::window::WindowBuilder;
use tauri::{AppHandle, LogicalSize, Manager, WindowEvent};

/// The interface method name this module handles.
pub const OPEN_METHOD: &str = "pluginEditor.open";
/// Top margin (logical pixels): the plugin view fills the window, and the top pixels go under the window title bar.
/// The plugin view sits at the bottom edge, so the extra height stays an empty strip above it.
const TITLE_BAR_PAD: f64 = 32.0;
/// How often open windows pull parameter values from the project.
const SYNC_INTERVAL: Duration = Duration::from_millis(100);

struct OpenWindow {
    id: EditorId,
    label: String,
    /// The size of the plugin interface the window shows now (logical pixels).
    size: (u32, u32),
}

pub struct PluginWindows {
    editors: Arc<dyn PluginEditors>,
    log: Arc<dyn AppLog>,
    open: Mutex<HashMap<EditorTarget, OpenWindow>>,
    /// The app, known once Tauri has started (`bind`): `flush` reaches the main thread through it.
    app: OnceLock<AppHandle>,
}

impl PluginWindows {
    pub fn new(editors: Arc<dyn PluginEditors>, log: Arc<dyn AppLog>) -> Self {
        Self { editors, log, open: Mutex::new(HashMap::new()), app: OnceLock::new() }
    }

    /// Tauri has started: from now on `flush` can run on the main thread.
    pub fn bind(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    fn windows(&self) -> MutexGuard<'_, HashMap<EditorTarget, OpenWindow>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Handles `pluginEditor.open` from the command thread: the work itself runs on the main thread, the answer is awaited here.
    pub fn handle_open(self: &Arc<Self>, app: &AppHandle, params: Value) -> Result<Value, ContractError> {
        let result = self.request_open(app, params);
        if let Err(error) = &result {
            self.log.record(LogEntry::warn("rpc", format!("{OPEN_METHOD}: {} {error}", error.code().code)));
        }
        result.map(|()| Value::Null)
    }

    fn request_open(self: &Arc<Self>, app: &AppHandle, params: Value) -> Result<(), ContractError> {
        let bad_params = |details: &dyn std::fmt::Display| rpc::BAD_PARAMS.with("method", OPEN_METHOD).with("details", details);
        let target = params.get("target").cloned().ok_or_else(|| bad_params(&"missing field `target`"))?;
        let target: EditorTarget = serde_json::from_value(target).map_err(|e| bad_params(&e))?;
        let (sender, receiver) = mpsc::channel();
        let (windows, handle) = (Arc::clone(self), app.clone());
        app.run_on_main_thread(move || {
            let _ = sender.send(windows.open_or_raise(&handle, target));
        })
        .map_err(window_failed)?;
        receiver.recv().map_err(window_failed)?
    }

    /// Main thread. If the target's window exists, raise it; otherwise create the plugin and the window and attach one to the other.
    fn open_or_raise(self: &Arc<Self>, app: &AppHandle, target: EditorTarget) -> Result<(), ContractError> {
        let existing = self.windows().get(&target).map(|w| (w.id, w.label.clone()));
        if let Some((id, label)) = existing {
            if let Some(window) = app.get_window(&label) {
                let _ = window.unminimize();
                window.show().map_err(window_failed)?;
                return window.set_focus().map_err(window_failed);
            }
            self.release(id);
        }
        let session = self.editors.open(target)?;
        let label = format!("plugin-editor-{}", session.id);
        let built = WindowBuilder::new(app, &label)
            .title(&session.title)
            .inner_size(f64::from(session.width), f64::from(session.height) + TITLE_BAR_PAD)
            .resizable(session.resizable)
            .build()
            .map_err(window_failed);
        let window = match built {
            Ok(window) => window,
            Err(error) => {
                self.editors.close(session.id);
                return Err(error);
            }
        };
        let attached = window.ns_view().map_err(window_failed).and_then(|view| self.editors.attach(session.id, NativeView(view.addr())));
        if let Err(error) = attached {
            self.editors.close(session.id);
            let _ = window.destroy();
            return Err(error);
        }
        self.windows().insert(target, OpenWindow { id: session.id, label, size: (session.width, session.height) });
        let (windows, handle, id) = (Arc::clone(self), app.clone(), session.id);
        window.on_window_event(move |event| windows.on_window_event(&handle, id, event));
        let _ = window.set_focus();
        Ok(())
    }

    /// Main thread: plugin window events.
    fn on_window_event(&self, app: &AppHandle, id: EditorId, event: &WindowEvent) {
        match event {
            WindowEvent::Resized(physical) => {
                let Some(label) = self.label_of(id) else { return };
                let scale = app.get_window(&label).and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
                let logical = physical.to_logical::<f64>(scale);
                let size = (logical.width.round() as u32, (logical.height - TITLE_BAR_PAD).round().max(0.0) as u32);
                self.apply_user_resize(id, size);
            }
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed => self.release(id),
            _ => {}
        }
    }

    fn label_of(&self, id: EditorId) -> Option<String> {
        self.windows().values().find(|w| w.id == id).map(|w| w.label.clone())
    }

    fn apply_user_resize(&self, id: EditorId, size: (u32, u32)) {
        {
            let mut windows = self.windows();
            let Some(window) = windows.values_mut().find(|w| w.id == id) else { return };
            if window.size == size || size.0 == 0 || size.1 == 0 {
                return;
            }
            window.size = size;
        }
        self.resize_plugin(id, size);
    }

    fn resize_plugin(&self, id: EditorId, size: (u32, u32)) {
        if let Err(error) = self.editors.resize(id, size.0, size.1) {
            self.note_failure(&format!("could not pass the new window size to the plugin: {error}"));
        }
    }

    /// Main thread, on a timer: each window's tick. The plugin's size request resizes the window; a vanished target closes it.
    fn tick(&self, app: &AppHandle) {
        let entries: Vec<(EditorTarget, EditorId, String, (u32, u32))> =
            self.windows().iter().map(|(target, w)| (*target, w.id, w.label.clone(), w.size)).collect();
        for (target, id, label, size) in entries {
            match self.editors.sync(id) {
                Ok(Some(wanted)) if wanted != size && wanted.0 > 0 && wanted.1 > 0 => {
                    if let Some(window) = app.get_window(&label) {
                        if let Some(open) = self.windows().get_mut(&target) {
                            open.size = wanted;
                        }
                        let _ = window.set_size(LogicalSize::new(f64::from(wanted.0), f64::from(wanted.1) + TITLE_BAR_PAD));
                        self.resize_plugin(id, wanted);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    self.note_failure(&format!("plugin window closed: {error}"));
                    self.release(id);
                    if let Some(window) = app.get_window(&label) {
                        let _ = window.destroy();
                    }
                }
            }
        }
    }

    /// Closes the plugin interface and forgets the window. Safe to call repeatedly.
    fn release(&self, id: EditorId) {
        let gone = {
            let mut windows = self.windows();
            let target = windows.iter().find(|(_, w)| w.id == id).map(|(t, _)| *t);
            target.and_then(|t| windows.remove(&t))
        };
        if gone.is_some() {
            self.editors.close(id);
        }
    }

    /// App exit: plugin interfaces are closed before everything else is unloaded.
    pub fn close_all(&self) {
        let ids: Vec<EditorId> = self.windows().values().map(|w| w.id).collect();
        for id in ids {
            self.release(id);
        }
    }

    fn has_open(&self) -> bool {
        !self.windows().is_empty()
    }

    fn note_failure(&self, message: &str) {
        self.log.record(LogEntry::warn("plugin-window", message));
    }
}

impl PluginStateFlush for PluginWindows {
    /// Plugins work on the main thread: the request goes there and the answer is awaited here. Without a bound app no
    /// window can be open yet.
    fn flush(&self) {
        let Some(app) = self.app.get().filter(|_| self.has_open()) else {
            return;
        };
        let (sender, receiver) = mpsc::channel();
        let editors = Arc::clone(&self.editors);
        let asked = app.run_on_main_thread(move || {
            editors.store_states();
            let _ = sender.send(());
        });
        if let Err(error) = asked.map_err(|e| e.to_string()).and_then(|()| receiver.recv().map_err(|e| e.to_string())) {
            self.note_failure(&format!("plugin window states were not written before saving: {error}"));
        }
    }
}

/// Window timer: every `SYNC_INTERVAL`, while there are open windows, asks the main thread for a tick.
pub fn spawn_sync_timer(app: AppHandle, windows: Arc<PluginWindows>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(SYNC_INTERVAL);
            if !windows.has_open() {
                continue;
            }
            let (timer_windows, handle) = (Arc::clone(&windows), app.clone());
            if app.run_on_main_thread(move || timer_windows.tick(&handle)).is_err() {
                return;
            }
        }
    });
}

fn window_failed(error: impl std::fmt::Display) -> ContractError {
    plugins::WINDOW_FAILED.with("details", error)
}
