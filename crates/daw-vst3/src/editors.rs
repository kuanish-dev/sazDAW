//! `Vst3Editors`: "plugin windows" — the `PluginEditors` implementation.
//!
//! For a target (an effect node or a track with an instrument) it finds the VST3 plugin, creates a separate instance for
//! the window and keeps it until closed. Which parameter edits go where is `ParamBridge`'s business, when to write the
//! window state is `StateSync`'s; here: bookkeeping of open windows and moving state between window and project (`PluginStates`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use daw_contracts::codes::plugins;
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, ContractError, EditorId, EditorSession, EditorTarget, ErrorKind, GraphControl, HistoryControl, LiveNoteSink, NativeView,
    PluginEditors, PluginState, PluginStates, TrackControl,
};

use crate::directory::PluginDirectory;
use crate::editor::{Editor, OpenError};
use crate::journal;
use crate::keys::EditorKeys;
use crate::param_bridge::ParamBridge;
use crate::project_params::{self, Target};
use crate::source::PluginSource;

pub struct Vst3Editors {
    directory: Arc<PluginDirectory>,
    graph: Arc<dyn GraphControl>,
    tracks: Arc<dyn TrackControl>,
    history: Arc<dyn HistoryControl>,
    states: Arc<dyn PluginStates>,
    log: Arc<dyn AppLog>,
    /// Computer keys in instrument windows play notes on the instrument's track.
    keys: EditorKeys,
    open: Mutex<HashMap<EditorId, OpenEditor>>,
    next_id: AtomicU32,
}

struct OpenEditor {
    target: EditorTarget,
    /// Plugin name, for the log.
    title: String,
    editor: Editor,
    /// The window's key in `EditorKeys`, if its keys are listened to.
    key_window: Option<usize>,
}

impl Vst3Editors {
    pub fn new(
        directory: Arc<PluginDirectory>,
        graph: Arc<dyn GraphControl>,
        tracks: Arc<dyn TrackControl>,
        history: Arc<dyn HistoryControl>,
        states: Arc<dyn PluginStates>,
        log: Arc<dyn AppLog>,
        live: Arc<dyn LiveNoteSink>,
    ) -> Self {
        let keys = EditorKeys::new(live, log.clone());
        Self { directory, graph, tracks, history, states, log, keys, open: Mutex::new(HashMap::new()), next_id: AtomicU32::new(1) }
    }

    fn source_of(&self, resolved: &Target) -> Result<Arc<PluginSource>, ContractError> {
        if !resolved.kind.starts_with("vst3:") {
            return Err(resolved.no_window.with("kind", &resolved.kind));
        }
        self.directory.find(&resolved.kind).ok_or_else(|| plugins::NOT_FOUND.with("kind", &resolved.kind))
    }

    fn build(&self, source: &PluginSource, resolved: &Target) -> Result<(Editor, String), ContractError> {
        let info = source.info();
        let fail = |text: String| {
            let message = source.describe_error(&text);
            journal::error(&*self.log, message.clone());
            plugins::WINDOW_FAILED.with("details", message)
        };
        let instance = source.instance().map_err(fail)?;
        let exposed: HashMap<u32, f64> = info.params.iter().map(|p| (p.id, p.default_normalized)).collect();
        let bridge = Arc::new(Mutex::new(ParamBridge::new(
            resolved.params.clone(),
            self.history.clone(),
            self.log.clone(),
            info.name.clone(),
            exposed,
            instance.parameters().scales,
        )));
        match Editor::create(instance, bridge) {
            Ok(editor) => Ok((editor, info.name.clone())),
            Err(OpenError::NoWindow) => Err(plugins::NO_EDITOR.with("name", &info.name)),
            Err(OpenError::Failed(text)) => Err(fail(text)),
        }
    }

    fn with_entry<R>(&self, id: EditorId, job: impl FnOnce(&mut OpenEditor) -> Result<R, ContractError>) -> Result<R, ContractError> {
        let mut open = lock(&self.open);
        job(open.get_mut(&id).ok_or_else(|| plugins::EDITOR_NOT_FOUND.with("id", id))?)
    }

    /// The window gets the target's state from the project (or its own initial one if the project has none), then the
    /// project's parameter values on top, and remembers this as agreed.
    fn adopt(&self, entry: &mut OpenEditor, project: Option<Arc<PluginState>>, now: Instant) -> Result<(), ContractError> {
        if let Some(state) = project.as_deref().or(entry.editor.initial.as_ref()).cloned()
            && let Err(error) = entry.editor.load(&state)
        {
            journal::error(&*self.log, format!("plugin window \"{}\": the saved state did not load: {error}", entry.title));
        }
        entry.editor.sync(now)?;
        let window = entry.editor.capture().ok();
        entry.editor.state().agreed(project, window);
        Ok(())
    }

    /// State tick: a state changed outside goes to the window; window changes after a pause go to the project.
    fn follow_state(&self, entry: &mut OpenEditor, now: Instant) -> Result<(), ContractError> {
        let project = self.states.get(entry.target)?;
        if entry.editor.state().outside(project.as_ref()) {
            return self.adopt(entry, project, now);
        }
        if entry.editor.state().due(now) {
            self.store(entry);
        }
        Ok(())
    }

    /// Writes the window state to the project if it differs from the agreed one. Failures go to the log.
    fn store(&self, entry: &mut OpenEditor) {
        let captured = match entry.editor.capture() {
            Ok(captured) => captured,
            Err(error) => {
                journal::error(&*self.log, format!("plugin window \"{}\": the plugin did not give its state: {error}", entry.title));
                entry.editor.state().settle();
                return;
            }
        };
        if !entry.editor.state().differs(&captured) {
            entry.editor.state().settle();
            return;
        }
        match self.states.set(entry.target, captured.clone()) {
            Ok(()) => entry.editor.state().agreed(Some(Arc::new(captured.clone())), Some(captured)),
            Err(error) if error.kind() == ErrorKind::NotFound => entry.editor.state().settle(),
            Err(error) => {
                journal::error(&*self.log, format!("plugin window \"{}\": the state was not written to the project: {error}", entry.title));
                entry.editor.state().settle();
            }
        }
    }
}

impl PluginEditors for Vst3Editors {
    fn open(&self, target: EditorTarget) -> Result<EditorSession, ContractError> {
        let resolved = project_params::resolve(target, &self.graph, &self.tracks)?;
        let source = self.source_of(&resolved)?;
        let mut open = lock(&self.open);
        if open.values().any(|entry| entry.target == target) {
            return Err(plugins::EDITOR_ALREADY_OPEN.err());
        }
        let saved = self.states.get(target)?;
        let (editor, title) = self.build(&source, &resolved)?;
        let mut entry = OpenEditor { target, title: title.clone(), editor, key_window: None };
        self.adopt(&mut entry, saved, Instant::now())?;
        let session = EditorSession {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            title,
            width: entry.editor.width,
            height: entry.editor.height,
            resizable: entry.editor.resizable,
        };
        open.insert(session.id, entry);
        Ok(session)
    }

    fn attach(&self, id: EditorId, parent: NativeView) -> Result<(), ContractError> {
        self.with_entry(id, |entry| {
            entry.editor.attach(parent.0)?;
            if let EditorTarget::Instrument { track } = entry.target {
                entry.key_window = self.keys.watch(parent.0, track);
            }
            Ok(())
        })
    }

    fn resize(&self, id: EditorId, width: u32, height: u32) -> Result<(), ContractError> {
        self.with_entry(id, |entry| entry.editor.resize(width, height))
    }

    fn sync(&self, id: EditorId) -> Result<Option<(u32, u32)>, ContractError> {
        let now = Instant::now();
        self.with_entry(id, |entry| {
            let size = entry.editor.sync(now)?;
            self.follow_state(entry, now)?;
            Ok(size)
        })
    }

    fn close(&self, id: EditorId) {
        let closed = lock(&self.open).remove(&id);
        if let Some(mut entry) = closed {
            if let Some(window) = entry.key_window {
                self.keys.unwatch(window);
            }
            self.store(&mut entry);
        }
    }

    fn store_states(&self) {
        let mut open = lock(&self.open);
        for entry in open.values_mut() {
            self.store(entry);
        }
    }
}

/// Whether this is the main thread: plugin windows and the key monitor may only be removed on it.
fn on_main_thread() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2::MainThreadMarker::new().is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

impl Drop for Vst3Editors {
    fn drop(&mut self) {
        if !on_main_thread() && (!lock(&self.open).is_empty() || self.keys.has_monitor()) {
            journal::error(
                &*self.log,
                "plugin windows dropped off the main thread: windows and the key monitor are kept until exit".into(),
            );
            std::mem::forget(std::mem::take(&mut *lock(&self.open)));
            self.keys.abandon_monitor();
            return;
        }
        let all: HashSet<EditorId> = lock(&self.open).keys().copied().collect();
        for id in all {
            self.close(id);
        }
    }
}
