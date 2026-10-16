//! The app's background services: audio output and autosave (the interval comes from the user preferences).
//! Roots start them from here and keep the handle alive; they know nothing of the implementations (cpal, files).

use std::sync::Arc;

use daw_audio_cpal::CpalAudio;
use daw_contracts::{AppLog, ContractError, LogEntry, StreamConfig};
use daw_persistence::Autosaver;

use crate::Session;

/// A running autosave; it stops when dropped.
pub struct AutosaveRun {
    _saver: Autosaver,
}

/// A running audio output; the sound stops when dropped.
pub struct AudioRun {
    audio: Arc<CpalAudio>,
    config: StreamConfig,
}

impl AudioRun {
    pub fn config(&self) -> StreamConfig {
        self.config
    }
}

impl Drop for AudioRun {
    fn drop(&mut self) {
        self.audio.stop_output();
    }
}

impl Session {
    /// Saves the project to the autosave folder while it has unsaved edits. How often (and whether at all) comes from
    /// the user preferences (`Preferences::autosave_seconds`, 30 s by default); a changed setting applies right away.
    pub fn start_autosave(&self) -> AutosaveRun {
        let controls = &self.controls;
        let preferences = controls.preferences.clone();
        AutosaveRun {
            _saver: Autosaver::start(
                controls.project.clone(),
                controls.view.clone(),
                controls.storage.clone(),
                controls.log.clone(),
                Box::new(move || preferences.get().autosave_interval()),
            ),
        }
    }

    /// Opens the audio output with the saved device choice and hands it the engine. After that the device is changed
    /// through `controls.audio`. The cpal stream lives in the `CpalAudio` host thread; here is only the handle.
    pub fn start_audio(self) -> Result<AudioRun, ContractError> {
        let config = self.audio.start_output(self.renderer)?;
        Ok(AudioRun { audio: self.audio, config })
    }
}

/// A panic in any thread goes to the error log (with place and backtrace), then the regular hook runs.
pub fn install_panic_hook(log: Arc<dyn AppLog>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = match info.payload().downcast_ref::<&str>() {
            Some(text) => (*text).to_string(),
            None => info.payload().downcast_ref::<String>().cloned().unwrap_or_else(|| "panic without a message".to_string()),
        };
        let place = info.location().map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
        log.record(
            LogEntry::error("panic", format!("{text} ({place})")).with_details(std::backtrace::Backtrace::force_capture().to_string()),
        );
        previous(info);
    }));
}
