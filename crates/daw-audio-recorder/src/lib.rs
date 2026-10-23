//! Audio recording from the input onto audio tracks. One job: "record an audio take from the input onto the armed
//! tracks". (Notes played live are recorded by another crate, `daw-recorder`.)
//!
//! Arming and per-track input channels live in `arming.rs`; they also go to the engine (`InputMonitor`), which keeps
//! the input open and monitors it on the tracks. A take: `start` checks the armed tracks, opens capture, starts the
//! transport if needed and spawns a background thread that drains frames from `AudioCapture` every ~20 ms. There is
//! one input for everyone: the thread keeps all of it (channels 1–2), and `stop` gives each track its channels, cuts
//! the latency and puts the audio into the project as one undo step (`take.rs`; channel split and alignment in
//! `split.rs`). A take is one straight run of time, so it holds the transport's song loop from start to stop
//! (`TransportControl::hold_song_loop`): playback runs past the song end instead of jumping back under the take. The
//! loop region still jumps back: then `passes.rs` cuts the take at the jumps, and every pass becomes a take of one clip
//! on the loop region (loop recording).
//!
//! Memory: audio accumulates in a `Vec<f32>` up to a limit (`Deps::max_samples`, by default ~20 minutes of stereo at
//! 48 kHz). At the limit capture and the transport we started stop and the event is logged; the accumulated audio is
//! not lost: `stop` saves it as usual.

mod arming;
mod passes;
mod split;
mod take;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use daw_contracts::codes::{audio, tracks};
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, AssetStore, AudioCapture, AudioDevices, AudioRecordControl, AudioRecordStatus, Clip, ClipControl, ContractError, EngineStats,
    HistoryControl, InputChannels, InputMonitor, LogEntry, LogLevel, ProjectIo, SampleCatalog, SampleWriter, StreamConfig, TakeControl,
    TempoControl, TrackControl, TrackId, TransportControl, TransportState,
};

use arming::Arming;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Memory for audio is reserved in chunks of this many seconds: doubling the capacity on a long recording would hold
/// almost twice the memory needed (hundreds of extra megabytes at the ~460 MB limit).
const RESERVE_SECONDS: usize = 10;

/// Everything the recorder needs from the rest of the app (interfaces only).
pub struct Deps {
    pub capture: Arc<dyn AudioCapture>,
    pub devices: Arc<dyn AudioDevices>,
    /// Plugin delay compensation: the app plays this much later, and the musician plays along with it.
    pub stats: Arc<dyn EngineStats>,
    /// The engine: which tracks are armed and which input channels they monitor.
    pub monitor: Arc<dyn InputMonitor>,
    pub transport: Arc<dyn TransportControl>,
    /// The tempo map: where the take lands and how many ticks it takes.
    pub tempo: Arc<dyn TempoControl>,
    pub clips: Arc<dyn ClipControl>,
    /// A recording over a clip becomes its new take.
    pub takes: Arc<dyn TakeControl>,
    pub tracks: Arc<dyn TrackControl>,
    pub samples: Arc<dyn SampleCatalog>,
    pub writer: Arc<dyn SampleWriter>,
    pub assets: Arc<dyn AssetStore>,
    pub project: Arc<dyn ProjectIo>,
    pub history: Arc<dyn HistoryControl>,
    pub log: Arc<dyn AppLog>,
    /// Limit of accumulated audio in `f32` values (frames × channels).
    pub max_samples: usize,
}

impl Deps {
    /// ~20 minutes of stereo at 48 kHz (about 460 MB).
    pub const DEFAULT_MAX_SAMPLES: usize = 20 * 60 * 48_000 * 2;
}

/// Take progress the interface sees (without tracks: their state is in `Arming`).
#[derive(Default, Clone, Copy)]
struct Progress {
    recording: bool,
    full: bool,
    seconds: f32,
    dropped_frames: u64,
}

struct Inner {
    deps: Deps,
    progress: Mutex<Progress>,
}

/// The running take.
struct Running {
    /// Tracks armed at start and their channels.
    tracks: Vec<(TrackId, InputChannels)>,
    start_tick: u32,
    /// The loop region at the start: playback jumps back from its end during the take (`passes.rs`).
    loop_region: Option<(u32, u32)>,
    /// The transport was stopped and we started it: it must be stopped at the end.
    started_transport: bool,
    format: StreamConfig,
    stop: Arc<AtomicBool>,
    worker: JoinHandle<Captured>,
}

/// What the background thread brought back.
struct Captured {
    data: Vec<f32>,
    /// The thread already stopped capture and transport (length limit): no need to repeat.
    released: bool,
}

pub struct AudioRecorder {
    inner: Arc<Inner>,
    running: Mutex<Option<Running>>,
    arming: Mutex<Arming>,
}

impl AudioRecorder {
    pub fn new(deps: Deps) -> Self {
        Self {
            inner: Arc::new(Inner { deps, progress: Mutex::new(Progress::default()) }),
            running: Mutex::new(None),
            arming: Mutex::new(Arming::default()),
        }
    }

    /// Changes a track arming or input: checks, then the change and handing the table to the engine.
    fn change_arming(&self, track: TrackId, change: impl FnOnce(&mut Arming)) -> Result<(), ContractError> {
        let running = lock(&self.running);
        self.inner.check_track(track)?;
        if running.is_some() {
            return Err(audio::ARMING_WHILE_RECORDING.err());
        }
        let mut arming = lock(&self.arming);
        change(&mut arming);
        self.inner.deps.monitor.set_track_inputs(arming.inputs());
        Ok(())
    }

    /// Armed audio tracks still in the project; removed ones are disarmed (in the engine too).
    fn armed_tracks(&self) -> Vec<(TrackId, InputChannels)> {
        let audio: Vec<TrackId> = self.inner.deps.tracks.tracks().iter().filter(|t| t.is_audio()).map(|t| t.id).collect();
        let mut arming = lock(&self.arming);
        arming.keep_only(|track| audio.contains(&track));
        self.inner.deps.monitor.set_track_inputs(arming.inputs());
        arming.armed()
    }
}

impl Inner {
    fn log(&self, level: LogLevel, message: String) {
        self.deps.log.record(LogEntry::new(level, "audio-record", message));
    }

    /// Stops capture, and the transport if we started it. Safe to call twice.
    fn release(&self, started_transport: bool) {
        self.deps.capture.stop();
        if started_transport {
            self.deps.transport.stop();
        }
    }

    fn check_track(&self, track: TrackId) -> Result<(), ContractError> {
        let found = self.deps.tracks.tracks().into_iter().find(|t| t.id == track);
        match found {
            None => Err(tracks::NOT_FOUND.with("id", track)),
            Some(t) if !t.is_audio() => Err(audio::NOT_AUDIO_TRACK.with("name", t.name)),
            Some(_) => Ok(()),
        }
    }

    /// The background loop: drains frames and updates progress until asked to stop or the memory limit is reached.
    fn capture_loop(&self, stop: &AtomicBool, format: StreamConfig, started_transport: bool) -> Captured {
        let channels = usize::from(format.channels.max(1));
        let step = RESERVE_SECONDS * format.sample_rate as usize * channels;
        let mut data: Vec<f32> = Vec::new();
        let mut released = false;
        while !stop.load(Ordering::Relaxed) {
            thread::sleep(POLL_INTERVAL);
            if data.capacity() - data.len() < step / 2 {
                data.reserve_exact(step);
            }
            self.deps.capture.drain(&mut data);
            *lock(&self.progress) = Progress {
                recording: true,
                full: released,
                seconds: (data.len() / channels) as f32 / format.sample_rate as f32,
                dropped_frames: self.deps.capture.dropped_frames(),
            };
            if !released && data.len() >= self.deps.max_samples {
                self.release(started_transport);
                released = true;
                lock(&self.progress).full = true;
                self.log(LogLevel::Error, "audio recording stopped at the length limit; the accumulated audio is saved on stop".into());
            }
        }
        Captured { data, released }
    }
}

impl AudioRecordControl for AudioRecorder {
    fn set_armed(&self, track: TrackId, armed: bool) -> Result<(), ContractError> {
        self.change_arming(track, |arming| arming.set_armed(track, armed))
    }

    fn set_input(&self, track: TrackId, input: InputChannels) -> Result<(), ContractError> {
        self.change_arming(track, |arming| arming.set_input(track, input))
    }

    fn start(&self) -> Result<(), ContractError> {
        let mut running = lock(&self.running);
        if running.is_some() {
            return Err(audio::RECORDING_ALREADY_RUNNING.err());
        }
        let tracks = self.armed_tracks();
        if tracks.is_empty() {
            return Err(audio::NO_ARMED_TRACKS.err());
        }
        let inner = &self.inner;
        if inner.deps.project.path().is_none() {
            return Err(audio::PROJECT_NOT_SAVED.err());
        }
        let started_transport = inner.deps.transport.state() != TransportState::Playing;
        let cue = inner.deps.transport.cue().filter(|_| started_transport);
        let start_tick = u32::try_from(cue.unwrap_or_else(|| inner.deps.transport.position_ticks())).unwrap_or(u32::MAX);
        let to_tick = |tick: u64| u32::try_from(tick).unwrap_or(u32::MAX);
        let loop_region = inner.deps.transport.loop_region().map(|(start, end)| (to_tick(start), to_tick(end)));
        let format = inner.deps.capture.start()?;
        inner.deps.transport.hold_song_loop(true);
        if started_transport {
            inner.deps.transport.play();
        }
        *lock(&inner.progress) = Progress { recording: true, ..Progress::default() };

        let stop = Arc::new(AtomicBool::new(false));
        let (worker_inner, worker_stop) = (inner.clone(), stop.clone());
        let spawned = thread::Builder::new()
            .name("daw-audio-record".into())
            .spawn(move || worker_inner.capture_loop(&worker_stop, format, started_transport));
        match spawned {
            Ok(worker) => {
                *running = Some(Running { tracks, start_tick, loop_region, started_transport, format, stop, worker });
                Ok(())
            }
            Err(error) => {
                inner.release(started_transport);
                inner.deps.transport.hold_song_loop(false);
                *lock(&inner.progress) = Progress::default();
                Err(audio::RECORD_THREAD_FAILED.with("details", error))
            }
        }
    }

    fn stop(&self) -> Result<Vec<Clip>, ContractError> {
        let mut running = lock(&self.running);
        let Some(take) = running.take() else { return Err(audio::NOT_RECORDING.err()) };
        let inner = &self.inner;
        take.stop.store(true, Ordering::Relaxed);
        let Ok(mut captured) = take.worker.join() else {
            inner.release(take.started_transport);
            inner.deps.transport.hold_song_loop(false);
            *lock(&inner.progress) = Progress::default();
            return Err(audio::RECORD_THREAD_CRASHED.err());
        };
        inner.deps.capture.drain(&mut captured.data);
        if !captured.released {
            inner.release(take.started_transport);
        }
        inner.deps.capture.drain(&mut captured.data);
        inner.deps.transport.hold_song_loop(false);
        let dropped = inner.deps.capture.dropped_frames();
        *lock(&inner.progress) = Progress::default();
        if dropped > 0 {
            inner.log(LogLevel::Warn, format!("audio recording lost {dropped} frames (the input queue overflowed)"));
        }
        inner.save_takes(&take.tracks, take.start_tick, take.loop_region, take.format, captured.data)
    }

    fn status(&self) -> AudioRecordStatus {
        let progress = *lock(&self.inner.progress);
        let level = self.inner.deps.devices.input_level();
        AudioRecordStatus {
            recording: progress.recording,
            full: progress.full,
            seconds: progress.seconds,
            left: level.left,
            right: level.right,
            dropped_frames: progress.dropped_frames,
            tracks: lock(&self.arming).states(level),
            monitor_problem: self.inner.deps.monitor.monitor_status().problem(),
        }
    }
}

impl Drop for AudioRecorder {
    /// The app closes mid-take: stop the thread and close the input; the audio is not saved (the project is closed).
    fn drop(&mut self) {
        if let Some(take) = lock(&self.running).take() {
            take.stop.store(true, Ordering::Relaxed);
            let _ = take.worker.join();
            self.inner.release(take.started_transport);
            self.inner.deps.transport.hold_song_loop(false);
        }
    }
}
