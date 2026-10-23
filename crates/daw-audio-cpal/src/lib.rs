//! Audio through cpal: input and output devices, buffer size, stream restart when the choice changes.
//! The only crate that knows about cpal. One type is visible outside: `CpalAudio` (contract `AudioDevices`).
//!
//! How it works: cpal streams cannot move between threads, so `CpalAudio` runs a dedicated host thread (`host/`)
//! and sends it commands over a channel. The engine lives outside the output stream (`Arc<Mutex<..>>`) and moves to
//! the new stream when the device changes. The input opens only on request (`watch_input` and friends), otherwise
//! macOS would ask for microphone access at startup.

mod backend;
mod capture;
mod choose;
mod faults;
mod host;
mod live;
mod peaks;
mod system;

#[cfg(test)]
mod tests;

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AudioCapture, AudioDeviceInfo, AudioDevices, AudioRenderer, AudioSettings, AudioSettingsStore, AudioStatus, ContractError,
    InputLevel, LiveInput, StreamConfig, codes,
};

use backend::{Backend, NoDevices};
use host::{Command, Host, Shared};
use system::SystemBackend;

pub struct CpalAudio {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl CpalAudio {
    /// Real system devices. Opens nothing until `start`/`apply`/`watch_input` is called.
    pub fn new(store: Arc<dyn AudioSettingsStore>, log: Arc<dyn AppLog>) -> Self {
        Self::with_backend(store, log, || Box::new(SystemBackend))
    }

    /// A host without devices for tests and scripts: lists are empty, nothing opens, the choice is still saved.
    pub fn without_devices(store: Arc<dyn AudioSettingsStore>, log: Arc<dyn AppLog>) -> Self {
        Self::with_backend(store, log, || Box::new(NoDevices))
    }

    /// `make` runs on the host thread, so the backend itself need not be `Send`.
    fn with_backend(
        store: Arc<dyn AudioSettingsStore>,
        log: Arc<dyn AppLog>,
        make: impl FnOnce() -> Box<dyn Backend> + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::new(store.load()));
        let (commands, receiver) = channel();
        let thread_shared = shared.clone();
        #[allow(
            clippy::expect_used,
            reason = "the system refuses to create a thread: an audio app cannot work anyway, so fail early and clearly"
        )]
        let thread = thread::Builder::new()
            .name("daw-audio-host".into())
            .spawn(move || host::run(Host::new(make(), store, log, thread_shared), receiver))
            .expect("failed to start the audio thread");
        Self { commands, shared, thread: Mutex::new(Some(thread)) }
    }

    /// Sends a command and waits for the answer; `None` means the host thread is gone (panicked).
    fn ask<T>(&self, command: impl FnOnce(Sender<T>) -> Command) -> Option<T> {
        let (reply, answer) = channel();
        self.commands.send(command(reply)).ok()?;
        answer.recv().ok()
    }

    fn dead() -> ContractError {
        codes::audio::HOST_NOT_RESPONDING.err()
    }

    /// Opens the output with the saved choice and hands it the engine. A missing device falls back to the system one with a log entry.
    pub fn start_output(&self, renderer: Box<dyn AudioRenderer>) -> Result<StreamConfig, ContractError> {
        self.ask(|reply| Command::Start(renderer, reply)).unwrap_or_else(|| Err(Self::dead()))
    }

    /// Closes input and output and releases the engine. If the host thread is gone, there is nothing to close.
    pub fn stop_output(&self) {
        let _ = self.ask(Command::Stop);
    }
}

impl Drop for CpalAudio {
    fn drop(&mut self) {
        let (closed, _) = channel();
        drop(std::mem::replace(&mut self.commands, closed));
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

impl AudioDevices for CpalAudio {
    fn outputs(&self) -> Vec<AudioDeviceInfo> {
        self.ask(Command::Outputs).unwrap_or_default()
    }

    fn inputs(&self) -> Vec<AudioDeviceInfo> {
        self.ask(Command::Inputs).unwrap_or_default()
    }

    fn settings(&self) -> AudioSettings {
        self.shared.settings()
    }

    fn status(&self) -> AudioStatus {
        self.shared.status()
    }

    fn apply(&self, settings: AudioSettings) -> Result<AudioStatus, ContractError> {
        self.ask(|reply| Command::Apply(settings, reply)).unwrap_or_else(|| Err(Self::dead()))
    }

    fn watch_input(&self, on: bool) -> Result<(), ContractError> {
        self.ask(|reply| Command::WatchInput(on, reply)).unwrap_or_else(|| Err(Self::dead()))
    }

    fn input_level(&self) -> InputLevel {
        self.shared.tap().level()
    }
}

impl AudioCapture for CpalAudio {
    fn start(&self) -> Result<StreamConfig, ContractError> {
        self.ask(Command::CaptureStart).unwrap_or_else(|| Err(Self::dead()))
    }

    fn drain(&self, out: &mut Vec<f32>) -> usize {
        self.shared.drain_capture(out)
    }

    fn stop(&self) {
        let _ = self.ask(Command::CaptureStop);
    }

    fn dropped_frames(&self) -> u64 {
        self.shared.tap().dropped()
    }

    fn take_live_input(&self) -> Option<Box<dyn LiveInput>> {
        self.shared.take_live_reader().map(|reader| Box::new(reader) as Box<dyn LiveInput>)
    }
}
