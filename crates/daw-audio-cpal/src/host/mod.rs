//! The audio host thread. A cpal stream cannot move between threads, so all output and input streams are created,
//! live and close on one dedicated thread; the rest of the code sends it commands.
//!
//! Changing the device or buffer = close the old stream, open a new one. The engine is not recreated: it lives in
//! `Arc<Mutex<..>>` outside the stream and moves to the new output stream. Output is in `output.rs`, input and capture
//! in `input.rs`, moving stream faults to the log and status in `faults.rs`.

mod faults;
mod input;
mod output;

use std::any::Any;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AudioDeviceInfo, AudioRenderer, AudioSettings, AudioSettingsStore, AudioStatus, ContractError, LogEntry, LogLevel,
    StreamConfig, codes,
};

use crate::backend::{Backend, SharedRenderer};
use crate::capture::{CaptureSource, InputTap};
use crate::choose::buffer_request;
use crate::live::{LiveReader, live_queue};
use faults::FaultWatch;

/// How often the idle host thread wakes up to move stream faults to the log and status.
const FAULT_CHECK: Duration = Duration::from_millis(250);

/// State read from any thread without asking the host thread. Only the host thread changes it.
pub(crate) struct Shared {
    settings: Mutex<AudioSettings>,
    status: Mutex<AudioStatus>,
    /// Peaks, the recording flag and the loss counter: shared with the input callback.
    tap: InputTap,
    /// The reading side of the capture queue. Outlives the input so the rest can still be drained.
    source: Mutex<Option<CaptureSource>>,
    /// The reading side of the monitoring input: waits until the engine takes it.
    live_reader: Mutex<Option<LiveReader>>,
}

impl Shared {
    pub fn new(settings: AudioSettings) -> Self {
        let (live, reader) = live_queue();
        Self {
            settings: Mutex::new(settings),
            status: Mutex::new(AudioStatus::default()),
            tap: InputTap::new(live),
            source: Mutex::new(None),
            live_reader: Mutex::new(Some(reader)),
        }
    }

    pub fn take_live_reader(&self) -> Option<LiveReader> {
        lock(&self.live_reader).take()
    }

    pub fn settings(&self) -> AudioSettings {
        lock(&self.settings).clone()
    }

    pub fn status(&self) -> AudioStatus {
        lock(&self.status).clone()
    }

    pub fn tap(&self) -> &InputTap {
        &self.tap
    }

    /// Moves the queued capture frames to the end of `out`; returns their number.
    pub fn drain_capture(&self, out: &mut Vec<f32>) -> usize {
        lock(&self.source).as_mut().map_or(0, |source| source.drain(out))
    }

    fn set_settings(&self, settings: AudioSettings) {
        *lock(&self.settings) = settings;
    }

    fn update_status(&self, change: impl FnOnce(&mut AudioStatus)) {
        change(&mut lock(&self.status));
    }

    fn set_source(&self, source: CaptureSource) {
        *lock(&self.source) = Some(source);
    }

    fn capture_format(&self) -> Option<StreamConfig> {
        lock(&self.source).as_ref().map(CaptureSource::format)
    }

    fn clear_capture(&self) {
        if let Some(source) = lock(&self.source).as_mut() {
            source.clear();
        }
    }
}

pub(crate) enum Command {
    Start(Box<dyn AudioRenderer>, Sender<Result<StreamConfig, ContractError>>),
    Stop(Sender<()>),
    Apply(AudioSettings, Sender<Result<AudioStatus, ContractError>>),
    WatchInput(bool, Sender<Result<(), ContractError>>),
    CaptureStart(Sender<Result<StreamConfig, ContractError>>),
    CaptureStop(Sender<()>),
    Outputs(Sender<Vec<AudioDeviceInfo>>),
    Inputs(Sender<Vec<AudioDeviceInfo>>),
}

pub(crate) struct Host {
    backend: Box<dyn Backend>,
    store: Arc<dyn AudioSettingsStore>,
    log: Arc<dyn AppLog>,
    shared: Arc<Shared>,
    renderer: Option<SharedRenderer>,
    /// Streams live while they are here; `None` means closed.
    output: Option<Box<dyn Any>>,
    input: Option<Box<dyn Any>>,
    output_faults: FaultWatch,
    input_faults: FaultWatch,
    /// The input is needed while at least one consumer holds it: the level meter or recording.
    watching: bool,
    capturing: bool,
    /// Monitoring holds the input (the engine asked, `LiveTap::wanted`).
    monitoring: bool,
    /// The monitoring input failed to open: already logged, waiting until the engine stops asking.
    monitor_failed: bool,
}

/// The host loop: runs while the command sender lives. Between commands it moves stream faults to the log.
pub(crate) fn run(mut host: Host, commands: Receiver<Command>) {
    loop {
        match commands.recv_timeout(FAULT_CHECK) {
            Ok(command) => host.execute(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        host.follow_monitoring();
        host.report_faults();
    }
}

impl Host {
    pub fn new(backend: Box<dyn Backend>, store: Arc<dyn AudioSettingsStore>, log: Arc<dyn AppLog>, shared: Arc<Shared>) -> Self {
        Self {
            backend,
            store,
            log,
            shared,
            renderer: None,
            output: None,
            input: None,
            output_faults: FaultWatch::new("output"),
            input_faults: FaultWatch::new("input"),
            watching: false,
            capturing: false,
            monitoring: false,
            monitor_failed: false,
        }
    }

    /// Replies are sent unchecked: if the caller is gone, there is nobody to answer.
    fn execute(&mut self, command: Command) {
        match command {
            Command::Start(renderer, reply) => _ = reply.send(self.start(renderer)),
            Command::Stop(reply) => {
                self.stop();
                _ = reply.send(());
            }
            Command::Apply(settings, reply) => _ = reply.send(self.apply(settings)),
            Command::WatchInput(on, reply) => _ = reply.send(self.watch_input(on)),
            Command::CaptureStart(reply) => _ = reply.send(self.capture_start()),
            Command::CaptureStop(reply) => {
                self.capture_stop();
                _ = reply.send(());
            }
            Command::Outputs(reply) => _ = reply.send(self.backend.outputs()),
            Command::Inputs(reply) => _ = reply.send(self.backend.inputs()),
        }
    }

    fn note(&self, level: LogLevel, message: String) {
        self.log.record(LogEntry::new(level, "audio", message));
    }

    /// Starts audio with the saved choice. A missing device is no reason to stay silent: play on the system device
    /// and warn in the log.
    pub fn start(&mut self, renderer: Box<dyn AudioRenderer>) -> Result<StreamConfig, ContractError> {
        self.renderer = Some(Arc::new(Mutex::new(renderer)));
        let saved = self.shared.settings();
        let mut result = self.open_output(saved.output.as_deref(), saved.buffer_frames);
        if let Err(error) = &result
            && (saved.output.is_some() || saved.buffer_frames.is_some())
        {
            self.note(LogLevel::Warn, format!("the saved audio choice did not open ({error}); playing on the system device"));
            result = self.open_output(None, None);
        }
        if let Err(error) = result {
            self.renderer = None;
            return Err(error);
        }
        let status = self.shared.status();
        Ok(StreamConfig { sample_rate: status.sample_rate, channels: status.channels })
    }

    /// Closes input and output and releases the engine: `Start` works again afterwards.
    pub fn stop(&mut self) {
        self.output = None;
        self.input = None;
        self.watching = false;
        self.capturing = false;
        self.monitoring = false;
        self.shared.tap.live().set_rate(0);
        self.shared.tap.end_capture();
        self.shared.tap.reset_level();
        self.renderer = None;
        self.shared.update_status(|status| status.running = false);
    }

    /// Returns audio to where it played before a failed attempt: the previous device, otherwise the system one.
    fn restore(&mut self, previous: &AudioSettings) {
        if self.open_output(previous.output.as_deref(), previous.buffer_frames).is_ok() {
            return;
        }
        self.note(LogLevel::Warn, "the previous output device did not open; playing on the system device".into());
        if let Err(error) = self.open_output(None, None) {
            self.note(LogLevel::Error, format!("the system output device did not open either, there is no sound: {error}"));
        }
    }

    /// Checks the choice before closing the running stream: an error must not leave the user without sound.
    fn check(&self, wanted: &AudioSettings) -> Result<(), ContractError> {
        if let Some(name) = &wanted.input
            && !self.backend.inputs().iter().any(|d| &d.name == name)
        {
            return Err(codes::audio::INPUT_DEVICE_NOT_FOUND.with("name", name));
        }
        if wanted.output.is_some() || wanted.buffer_frames.is_some() {
            let info = self.backend.probe_output(wanted.output.as_deref())?;
            buffer_request(wanted.buffer_frames, info.buffer_range)?;
        }
        Ok(())
    }

    pub fn apply(&mut self, wanted: AudioSettings) -> Result<AudioStatus, ContractError> {
        self.check(&wanted)?;
        let previous = self.shared.settings();
        if self.capturing && wanted.input != previous.input {
            return Err(codes::audio::INPUT_CHANGE_WHILE_RECORDING.err());
        }
        if self.renderer.is_some()
            && let Err(error) = self.open_output(wanted.output.as_deref(), wanted.buffer_frames)
        {
            self.restore(&previous);
            return Err(error);
        }
        if let Err(error) = self.store.save(&wanted) {
            if self.renderer.is_some() {
                self.restore(&previous);
            }
            return Err(error);
        }
        self.shared.set_settings(wanted);
        self.reopen_input_if_watched();
        Ok(self.shared.status())
    }
}
