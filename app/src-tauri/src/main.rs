//! The desktop root of the app: picks the audio output and autosave, and starts the window.
//! The implementations are assembled by `daw-bootstrap`; the interface only gets `daw-rpc` commands (JSON, and bytes for
//! the live sound windows) and two event streams: `tick` and the project feed (`project`).

mod ai_server;
mod plugin_window;

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use daw_bootstrap::{AutosaveRun, SessionConfig, build_session_in, default_state_dir, install_panic_hook};
use daw_contracts::codes::rpc;
use daw_contracts::{ContractError, Controls, LogEntry};
use daw_rpc::{ProjectFeed, RpcTable, WholeRequest};
use plugin_window::{OPEN_METHOD, PluginWindows};
use serde_json::Value;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, Manager, State};

const TICK_INTERVAL: Duration = Duration::from_millis(33);

struct AppState {
    controls: Controls,
    table: RpcTable,
    /// Whether the start project is open: until then interface calls wait, while the window already shows loading.
    startup: Arc<(Mutex<bool>, Condvar)>,
    /// Keeps autosave alive until exit.
    autosaver: Mutex<Option<AutosaveRun>>,
    /// Plugin windows (native windows with the VST3 plugin's own interface).
    plugin_windows: Arc<PluginWindows>,
    /// The AI server (MCP over HTTP): listens while the app is alive.
    ai_server: Arc<Mutex<Option<daw_mcp::HttpEndpoint>>>,
    /// Where the window asks the project feed for the whole project (`project_whole`).
    whole_project: WholeRequest,
}

/// The interface's command: calls a `daw-rpc` method by name.
/// Heavy operations (export) run on a separate thread and do not block the window.
/// The only exception is `pluginEditor.open`: it needs Tauri windows, and `daw-rpc` knows nothing about Tauri. Before a
/// method that saves or renders the project, the table itself asks the plugin windows for their state (`PluginStateFlush`).
#[tauri::command]
async fn rpc(app: AppHandle, state: State<'_, Arc<AppState>>, method: String, params: Value) -> Result<Value, ContractError> {
    let state = Arc::clone(&state);
    let name = method.clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.wait_for_startup();
        if method == OPEN_METHOD {
            return state.plugin_windows.handle_open(&app, params);
        }
        state.table.call(&state.controls, &method, params)
    })
    .await
    .map_err(|e| rpc::TASK_FAILED.with("method", name).with("details", e))?
}

/// The same for the methods whose reply the interface reads many times a second (`RpcTable::call_binary`): the reply
/// comes as raw bytes, an `ArrayBuffer` on the other side, with no JSON in between.
#[tauri::command]
async fn rpc_binary(state: State<'_, Arc<AppState>>, method: String, params: Value) -> Result<Response, ContractError> {
    let state = Arc::clone(&state);
    let name = method.clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.wait_for_startup();
        state.table.call_binary(&state.controls, &method, params).map(Response::new)
    })
    .await
    .map_err(|e| rpc::TASK_FAILED.with("method", name).with("details", e))?
}

/// The window asks for the whole project with the next `project` event: when it starts (its copy is empty) and after a
/// refused call (its copy may show a value the backend did not take). Like any call, it waits for the start project.
#[tauri::command]
async fn project_whole(state: State<'_, Arc<AppState>>) -> Result<(), ContractError> {
    let state = Arc::clone(&state);
    tauri::async_runtime::spawn_blocking(move || {
        state.wait_for_startup();
        state.whole_project.ask();
    })
    .await
    .map_err(|e| rpc::TASK_FAILED.with("method", "project_whole").with("details", e))
}

impl AppState {
    /// Interface calls wait until the start project is open.
    fn wait_for_startup(&self) {
        let (lock, signal) = &*self.startup;
        let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        drop(signal.wait_while(guard, |open| !*open).unwrap_or_else(PoisonError::into_inner));
    }
}

/// Lets the waiting interface calls through when dropped: after the start project opened, and also when opening
/// panicked (the hook logs the panic), so the window never waits on "loading" forever.
struct StartupDone(Arc<(Mutex<bool>, Condvar)>);

impl Drop for StartupDone {
    fn drop(&mut self) {
        let (lock, signal) = &*self.0;
        *lock.lock().unwrap_or_else(PoisonError::into_inner) = true;
        signal.notify_all();
    }
}

/// Every ~33 ms sends the interface the position and meters (`tick`) and, after project edits, what changed (`project`).
fn spawn_telemetry(app: AppHandle, controls: Controls, mut feed: ProjectFeed) {
    thread::spawn(move || {
        loop {
            if app.emit("tick", daw_rpc::tick(&controls)).is_err() {
                return;
            }
            // The window is closing: nobody can receive the event, and the next `tick` above ends the thread.
            if let Some(update) = feed.poll(&controls) {
                let _ = app.emit("project", update);
            }
            thread::sleep(TICK_INTERVAL);
        }
    });
}

#[allow(clippy::expect_used, reason = "a window start failure is fatal; the panic goes to the log")]
fn main() {
    let session = build_session_in(&SessionConfig::new(default_state_dir()));
    let opener = session.startup();
    let controls = session.controls.clone();
    install_panic_hook(controls.log.clone());
    let autosaver = session.start_autosave();

    // The audio output lives while its handle lives; without sound the window still opens.
    let log = controls.log.clone();
    let _audio = session.start_audio().map_err(|error| log.record(LogEntry::error("audio", format!("could not open audio: {error}")))).ok();

    // Both doors (the window and the AI server) ask the plugin windows before saving or rendering the project.
    let plugin_windows = Arc::new(PluginWindows::new(controls.plugin_editors.clone(), controls.log.clone()));
    let mut table = RpcTable::new();
    table.set_plugin_windows(plugin_windows.clone());

    // Start = the last open project, if any.
    // Runs in the background: the window opens right away and shows a loading indicator.
    let startup = Arc::new((Mutex::new(false), Condvar::new()));
    let ai_server = Arc::new(Mutex::new(None));
    {
        let startup = Arc::clone(&startup);
        let (ai_slot, ai_controls, ai_windows) = (Arc::clone(&ai_server), controls.clone(), plugin_windows.clone());
        thread::spawn(move || {
            let _done = StartupDone(startup);
            // If there is nothing to open, the window shows the welcome screen: the user creates a project.
            opener.open();
            // The AI server starts after the project opens: AI must not edit a project that is still loading.
            *ai_slot.lock().unwrap_or_else(PoisonError::into_inner) = ai_server::start(&ai_controls, ai_windows);
        });
    }

    let feed = ProjectFeed::new();
    let state = Arc::new(AppState {
        controls: controls.clone(),
        table,
        startup: Arc::clone(&startup),
        autosaver: Mutex::new(Some(autosaver)),
        plugin_windows: Arc::clone(&plugin_windows),
        ai_server,
        whole_project: feed.whole_request(),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::clone(&state))
        .invoke_handler(tauri::generate_handler![rpc, rpc_binary, project_whole])
        .setup(move |app| {
            plugin_windows.bind(app.handle().clone());
            spawn_telemetry(app.handle().clone(), controls.clone(), feed);
            plugin_window::spawn_sync_timer(app.handle().clone(), Arc::clone(&plugin_windows));
            Ok(())
        })
        .build(tauri::generate_context!())
        // The window was not created — nothing to work with: fail right away (the panic reaches the log through install_panic_hook).
        .expect("the app failed to start")
        .run(|app, event| {
            // Exit (including Cmd+Q on macOS, where the window is not "destroyed"): stop autosave,
            // it removes its file if there are no unsaved edits.
            match event {
                // The main window was closed: plugin windows are useless without it (the app used to exit when the last window closed).
                tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::Destroyed, .. } if label == "main" => app.exit(0),
                tauri::RunEvent::Exit => {
                    let state = app.state::<Arc<AppState>>();
                    // Plugin interfaces are closed before everything else is unloaded.
                    state.plugin_windows.close_all();
                    *state.autosaver.lock().unwrap_or_else(PoisonError::into_inner) = None;
                    *state.ai_server.lock().unwrap_or_else(PoisonError::into_inner) = None;
                    // Last: this is a normal exit, the next start must not take it for a plugin crash.
                    state.controls.plugin_crash.end_session();
                }
                _ => {}
            }
        });
}
