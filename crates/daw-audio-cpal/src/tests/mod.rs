//! Host thread tests on fake audio: no sound card needed. Here are the fake devices, store, log and engine; the checks
//! live in one file per job.

mod capture;
mod facade;
mod input;
mod monitor;
mod output;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use daw_contracts::codes::{audio, general};
use daw_contracts::{
    AppLog, AudioDeviceInfo, AudioDevices, AudioRenderer, AudioSettings, AudioSettingsStore, ContractError, LogEntry, LogLevel,
    StreamConfig,
};

use crate::CpalAudio;
use crate::backend::{Backend, OpenInput, OpenOutput, OutputInfo, SharedRenderer, render_block};
use crate::capture::{InputFeed, InputTap, input_queue};
use crate::faults::{Fault, FaultSink};
use crate::host::{Host, Shared};

type Events = Arc<Mutex<Vec<String>>>;

/// Fake hardware: output devices with different rates, two inputs, a journal of opens and closes.
#[derive(Default, Clone)]
struct Fake {
    events: Events,
    /// Devices that fail to open.
    broken: Arc<Mutex<Vec<String>>>,
    faults: Arc<Mutex<Option<FaultSink>>>,
    /// The "audio callback" of the last opened input: the test feeds frames into it.
    feed: Arc<Mutex<Option<InputFeed>>>,
}

struct Token {
    events: Events,
    what: String,
}

impl Drop for Token {
    fn drop(&mut self) {
        self.events.lock().unwrap().push(format!("close {}", self.what));
    }
}

fn info(name: &str, is_default: bool) -> AudioDeviceInfo {
    AudioDeviceInfo { name: name.into(), is_default, channels: 2 }
}

impl Fake {
    fn log(&self, text: String) {
        self.events.lock().unwrap().push(text);
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn rate(name: &str) -> u32 {
        if name == "Interface" { 96_000 } else { 48_000 }
    }

    fn resolve(device: Option<&str>) -> Result<&'static str, ContractError> {
        match device {
            None | Some("Speakers") => Ok("Speakers"),
            Some("Interface") => Ok("Interface"),
            Some("Broken") => Ok("Broken"),
            Some(other) => Err(audio::OUTPUT_DEVICE_NOT_FOUND.with("name", other)),
        }
    }
}

impl Backend for Fake {
    fn outputs(&self) -> Vec<AudioDeviceInfo> {
        vec![info("Speakers", true), info("Interface", false), info("Broken", false)]
    }

    fn inputs(&self) -> Vec<AudioDeviceInfo> {
        vec![info("Mic", true), info("Line", false)]
    }

    fn probe_output(&self, device: Option<&str>) -> Result<OutputInfo, ContractError> {
        Ok(OutputInfo { name: Self::resolve(device)?.into(), buffer_range: Some((32, 2048)) })
    }

    fn open_output(
        &self,
        device: Option<&str>,
        buffer_frames: Option<u32>,
        renderer: SharedRenderer,
        faults: FaultSink,
    ) -> Result<OpenOutput, ContractError> {
        let name = Self::resolve(device)?;
        if self.broken.lock().unwrap().iter().any(|b| b == name) {
            return Err(audio::OUTPUT_FAILED.with("details", format!("{name} is busy")));
        }
        let config = StreamConfig { sample_rate: Self::rate(name), channels: 2 };
        renderer.lock().unwrap().prepare(config);
        *self.faults.lock().unwrap() = Some(faults);
        self.log(format!("open {name} {buffer_frames:?}"));
        Ok(OpenOutput {
            stream: Box::new(Token { events: self.events.clone(), what: name.into() }),
            config,
            info: OutputInfo { name: name.into(), buffer_range: Some((32, 2048)) },
        })
    }

    fn open_input(&self, device: Option<&str>, tap: InputTap, faults: FaultSink) -> Result<OpenInput, ContractError> {
        if device == Some("Line") && self.broken.lock().unwrap().iter().any(|b| b == "Line") {
            return Err(audio::INPUT_FAILED.with("details", "access denied"));
        }
        *self.faults.lock().unwrap() = Some(faults);
        self.log(format!("open-in {device:?}"));
        let (feed, source) = input_queue(tap, 48_000, 2);
        *self.feed.lock().unwrap() = Some(feed);
        Ok(OpenInput { stream: Box::new(Token { events: self.events.clone(), what: format!("in {device:?}") }), source })
    }
}

#[derive(Default)]
struct MemoryStore {
    saved: Mutex<Option<AudioSettings>>,
    fail: Mutex<bool>,
}

impl AudioSettingsStore for MemoryStore {
    fn load(&self) -> AudioSettings {
        self.saved.lock().unwrap().clone().unwrap_or_default()
    }

    fn save(&self, settings: &AudioSettings) -> Result<(), ContractError> {
        if *self.fail.lock().unwrap() {
            return Err(general::INTERNAL.with("details", "disk full"));
        }
        *self.saved.lock().unwrap() = Some(settings.clone());
        Ok(())
    }
}

#[derive(Default)]
struct MemoryLog(Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }

    fn recent(&self, _limit: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }

    fn file(&self) -> Option<PathBuf> {
        None
    }
}

/// A dummy engine: remembers the configs it was prepared with.
struct Prepared(Arc<Mutex<Vec<StreamConfig>>>);

impl AudioRenderer for Prepared {
    fn prepare(&mut self, config: StreamConfig) {
        self.0.lock().unwrap().push(config);
    }

    fn render(&mut self, interleaved: &mut [f32]) {
        interleaved.fill(0.25);
    }
}

struct Rig {
    host: Host,
    fake: Fake,
    store: Arc<MemoryStore>,
    log: Arc<MemoryLog>,
    shared: Arc<Shared>,
    prepared: Arc<Mutex<Vec<StreamConfig>>>,
}

fn rig(saved: Option<AudioSettings>) -> Rig {
    let (fake, log, store) = (Fake::default(), Arc::new(MemoryLog::default()), Arc::new(MemoryStore::default()));
    *store.saved.lock().unwrap() = saved;
    let shared = Arc::new(Shared::new(store.load()));
    let host = Host::new(Box::new(fake.clone()), store.clone(), log.clone(), shared.clone());
    Rig { host, fake, store, log, shared, prepared: Arc::default() }
}

impl Rig {
    fn start(&mut self) -> Result<StreamConfig, ContractError> {
        self.host.start(Box::new(Prepared(self.prepared.clone())))
    }

    fn choose(output: Option<&str>, buffer: Option<u32>) -> AudioSettings {
        AudioSettings { output: output.map(Into::into), input: None, buffer_frames: buffer, record_latency_ms: None }
    }
}
