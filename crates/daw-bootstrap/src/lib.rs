//! Session assembly: the only place that knows the concrete implementations (the wiring is in `wiring/`).
//! App roots (console, desktop, windowless MCP) take a ready `Session` and work with the contracts.

mod clip_audio_watch;
mod engine_watch;
mod engines;
mod plugins;
mod services;
mod wiring;

pub use services::{AudioRun, AutosaveRun, install_panic_hook};

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use daw_audio_cpal::CpalAudio;
use daw_contracts::{AudioRenderer, Controls, LogEntry};

use wiring::Devices;

pub struct Session {
    pub controls: Controls,
    /// Goes to the audio output (`start_audio`).
    pub renderer: Box<dyn AudioRenderer>,
    /// The audio host: the same as in `controls.audio`, but with a start method the devices contract does not have.
    audio: Arc<CpalAudio>,
}

/// Where the app keeps its state (the recent projects list) and where the plugin scan looks.
pub struct SessionConfig {
    pub state_dir: PathBuf,
    /// Folders the plugin scan looks in. `None`: the system VST3 folders in a real session, nothing in a headless one.
    /// Tests set it to the dev-only fixture plugins (`test-plugins/*/target/bundled`).
    pub plugin_dirs: Option<Vec<PathBuf>>,
}

impl SessionConfig {
    /// State in `state_dir`, plugins from the default folders.
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir, plugin_dirs: None }
    }
}

/// The app's default state folder.
pub fn default_state_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
    if cfg!(target_os = "macos") { home.join("Library/Application Support/DAW") } else { home.join(".daw") }
}

/// The state folder of the windowless MCP server: its own, so it does not share autosaves and recent projects with the app window.
pub fn default_mcp_state_dir() -> PathBuf {
    default_state_dir().with_file_name("DAW-MCP")
}

/// Opening the startup project; separate from `Session` so it can run in the background while the window is already shown.
#[derive(Clone)]
pub struct Startup {
    controls: Controls,
}

impl Startup {
    /// Opens the last project from the recent list if this is enabled in the preferences ("Profile"); missing or
    /// broken ones are skipped and forgotten. `None` means there is nothing to open (disabled, first start or all gone).
    /// If the previous session broke off inside a plugin, the project is NOT opened: the same plugin would crash the app
    /// again before the user could turn it off (`PluginCrashGuard`); the user opens the project after answering the report.
    pub fn open(&self) -> Option<PathBuf> {
        let controls = &self.controls;
        if !controls.preferences.get().profile.open_last_project {
            return None;
        }
        if let Some(crash) = controls.plugin_crash.last_crash() {
            let message = format!("the last project was not opened: the previous session broke off in plugin \"{}\"", crash.name);
            controls.log.record(LogEntry::warn("startup", message));
            return None;
        }
        for file in controls.recent.list() {
            match controls.project.load(&file) {
                Ok(()) => return Some(file),
                Err(error) => {
                    controls.log.record(LogEntry::error("startup", format!("could not open {}: {error}", file.display())));
                    controls.recent.forget(&file);
                }
            }
        }
        None
    }
}

impl Session {
    pub fn startup(&self) -> Startup {
        Startup { controls: self.controls.clone() }
    }
}

static TEMP_STATE: AtomicU32 = AtomicU32::new(0);

/// A session for tests and scripts: a temporary state folder, no sound card;
/// `with_demo` fills the untitled project with a beat.
#[allow(clippy::expect_used, reason = "helper for tests and scripts: a failing demo build is a programming error")]
pub fn build_session(with_demo: bool) -> Session {
    let state_dir = std::env::temp_dir().join(format!("daw-state-{}-{}", std::process::id(), TEMP_STATE.fetch_add(1, Ordering::Relaxed)));
    let session = build_headless_session_in(&SessionConfig::new(state_dir));
    if with_demo {
        session.controls.demo.fill_demo().expect("could not build the starter project");
    }
    session
}

/// Builds a session with an empty project and the real system sound devices.
pub fn build_session_in(config: &SessionConfig) -> Session {
    wiring::build(config, &Devices::System)
}

/// Like `build_session_in`, but without a sound card and third-party plugins: for tests with a given state folder.
pub fn build_headless_session_in(config: &SessionConfig) -> Session {
    wiring::build(config, &Devices::Headless)
}
