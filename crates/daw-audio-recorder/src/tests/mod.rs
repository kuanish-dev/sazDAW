//! Recorder tests on fakes: no sound card, no project, no disk.
//! Here are the fakes and the recorder "world"; the checks live in one file per job.

mod lifecycle;
mod take;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use daw_contracts::codes::{audio, clips, general, tracks};
use daw_contracts::{
    AppLog, AssetStore, AudioCapture, AudioDeviceInfo, AudioDevices, AudioRecordControl, AudioSettings, AudioStatus, Bpm, Clip,
    ClipContent, ClipControl, ClipEdit, ClipId, CompSegment, ContractError, EngineLatency, EngineLosses, EngineStats, HistoryControl,
    HistoryMark, InputChannels, InputLevel, InputMonitor, InstrumentSpec, LiveInput, LogEntry, LogLevel, MonitorMode, MonitorStatus,
    NewNoteClip, ProjectIo, SampleCatalog, SampleData, SampleId, SampleInfo, SampleWriter, SignatureChange, StreamConfig, TakeControl,
    TempoChange, TempoControl, TempoMap, TimeSignature, Track, TrackControl, TrackId, TrackInput, TransportControl, TransportState,
};

use crate::{AudioRecorder, Deps};

type Events = Arc<Mutex<Vec<String>>>;

const AUDIO_TRACK: TrackId = 1;
const SYNTH_TRACK: TrackId = 2;
const BASS_TRACK: TrackId = 3;
const FORMAT: StreamConfig = StreamConfig { sample_rate: 48_000, channels: 2 };

struct FakeCapture {
    events: Events,
    pending: Mutex<Vec<f32>>,
    fail_start: Mutex<bool>,
    dropped: AtomicU64,
}

impl FakeCapture {
    fn push(&self, samples: &[f32]) {
        self.pending.lock().unwrap().extend_from_slice(samples);
    }
}

impl AudioCapture for FakeCapture {
    fn start(&self) -> Result<StreamConfig, ContractError> {
        if *self.fail_start.lock().unwrap() {
            return Err(audio::NO_INPUT_DEVICE.err());
        }
        self.events.lock().unwrap().push("capture.start".into());
        Ok(FORMAT)
    }

    fn drain(&self, out: &mut Vec<f32>) -> usize {
        let mut pending = self.pending.lock().unwrap();
        let frames = pending.len() / 2;
        out.append(&mut pending);
        frames
    }

    fn stop(&self) {
        self.events.lock().unwrap().push("capture.stop".into());
    }

    fn dropped_frames(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    fn take_live_input(&self) -> Option<Box<dyn LiveInput>> {
        None
    }
}

/// The engine as the recorder sees it: the last table of track inputs.
#[derive(Default)]
struct FakeMonitor {
    inputs: Mutex<Vec<TrackInput>>,
    status: Mutex<MonitorStatus>,
}

impl InputMonitor for FakeMonitor {
    fn set_track_inputs(&self, inputs: &[TrackInput]) {
        *self.inputs.lock().unwrap() = inputs.to_vec();
    }
    fn monitor_status(&self) -> MonitorStatus {
        *self.status.lock().unwrap()
    }
}

struct FakeDevices {
    settings: Mutex<AudioSettings>,
    level: Mutex<InputLevel>,
}

/// The engine as the recorder sees it: only the compensation latency (frames at 48 kHz).
#[derive(Default)]
struct FakeStats {
    latency: AtomicU64,
}

impl EngineStats for FakeStats {
    fn cpu_load(&self) -> f32 {
        0.0
    }
    fn meters(&self) -> Vec<daw_contracts::Meter> {
        Vec::new()
    }
    fn gain_reductions(&self) -> Vec<daw_contracts::GainReduction> {
        Vec::new()
    }
    fn preview_position(&self) -> Option<usize> {
        None
    }
    fn losses(&self) -> EngineLosses {
        EngineLosses::default()
    }
    fn latency(&self) -> EngineLatency {
        EngineLatency { frames: self.latency.load(Ordering::Relaxed) as u32, sample_rate: 48_000 }
    }
    fn watch_master(&self, _: bool) {}
    fn master_window(&self) -> daw_contracts::MasterWindow {
        daw_contracts::MasterWindow { samples: Vec::new(), sample_rate: 48_000 }
    }
}

impl AudioDevices for FakeDevices {
    fn outputs(&self) -> Vec<AudioDeviceInfo> {
        Vec::new()
    }
    fn inputs(&self) -> Vec<AudioDeviceInfo> {
        Vec::new()
    }
    fn settings(&self) -> AudioSettings {
        self.settings.lock().unwrap().clone()
    }
    fn status(&self) -> AudioStatus {
        AudioStatus { running: true, sample_rate: 48_000, channels: 2, ..AudioStatus::default() }
    }
    fn apply(&self, _: AudioSettings) -> Result<AudioStatus, ContractError> {
        Ok(self.status())
    }
    fn watch_input(&self, _: bool) -> Result<(), ContractError> {
        Ok(())
    }
    fn input_level(&self) -> InputLevel {
        *self.level.lock().unwrap()
    }
}

struct FakeTransport {
    events: Events,
    state: Mutex<TransportState>,
    position: AtomicU64,
    /// Song loop holds minus releases: below zero would mean an unpaired release.
    holds: Mutex<i32>,
    loop_region: Mutex<Option<(u64, u64)>>,
}

impl TransportControl for FakeTransport {
    fn play(&self) {
        *self.state.lock().unwrap() = TransportState::Playing;
        self.events.lock().unwrap().push("transport.play".into());
    }
    fn pause(&self) {}
    fn stop(&self) {
        *self.state.lock().unwrap() = TransportState::Stopped;
        self.events.lock().unwrap().push("transport.stop".into());
    }
    fn seek(&self, tick: u64) {
        self.position.store(tick, Ordering::Relaxed);
    }
    fn set_cue(&self, _: Option<u64>) {}
    fn cue(&self) -> Option<u64> {
        None
    }
    fn set_loop(&self, region: Option<(u64, u64)>) {
        *self.loop_region.lock().unwrap() = region;
    }
    fn loop_region(&self) -> Option<(u64, u64)> {
        *self.loop_region.lock().unwrap()
    }
    fn set_song_loop(&self, _: bool) {}
    fn song_loop(&self) -> bool {
        true
    }
    fn set_song_end(&self, _: Option<u64>) {}
    fn hold_song_loop(&self, hold: bool) {
        *self.holds.lock().unwrap() += if hold { 1 } else { -1 };
    }
    fn wrap_region(&self) -> Option<(u64, u64)> {
        None
    }
    fn state(&self) -> TransportState {
        *self.state.lock().unwrap()
    }
    fn position_ticks(&self) -> u64 {
        self.position.load(Ordering::Relaxed)
    }
}

/// 120 BPM, 4/4, no changes; the recorder needs no tempo edits.
struct FakeTempo;

impl TempoControl for FakeTempo {
    fn tempo_map(&self) -> TempoMap {
        TempoMap::constant(Bpm::new(120.0).unwrap(), TimeSignature::default())
    }
    fn set_tempo_changes(&self, _: Vec<TempoChange>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_signature_changes(&self, _: Vec<SignatureChange>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_tempo_map(&self, _: TempoMap) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_tempo(&self, _: Bpm) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_time_signature(&self, _: TimeSignature) -> Result<(), ContractError> {
        Err(unused())
    }
}

struct FakeTracks;

impl TrackControl for FakeTracks {
    fn tracks(&self) -> Vec<Track> {
        vec![
            Track::new(AUDIO_TRACK, "Vocals", None, 60),
            Track::new(BASS_TRACK, "Bass", None, 60),
            Track::new(SYNTH_TRACK, "Synth", Some(InstrumentSpec::new("synth", Default::default())), 60),
        ]
    }
    fn add_tracks(&self, _: Vec<daw_contracts::NewTrack>) -> Result<Vec<TrackId>, ContractError> {
        Err(unused())
    }
    fn add_track(&self, _: &str, _: Option<&str>, _: u8) -> Result<TrackId, ContractError> {
        Err(unused())
    }
    fn remove_track(&self, _: TrackId) -> Result<(), ContractError> {
        Ok(())
    }
    fn rename_track(&self, _: TrackId, _: &str) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_instrument_param(&self, _: TrackId, _: &str, _: f32) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_instrument(&self, _: TrackId, _: &str, _: daw_contracts::Params, _: Option<&str>) -> Result<(), ContractError> {
        Ok(())
    }
    fn move_track(&self, _: TrackId, _: usize) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_monitor(&self, _: TrackId, _: MonitorMode) -> Result<(), ContractError> {
        Ok(())
    }
    fn set_color(&self, _: TrackId, _: Option<&str>) -> Result<(), ContractError> {
        Err(unused())
    }
    fn set_height(&self, _: TrackId, _: Option<u16>) -> Result<(), ContractError> {
        Err(unused())
    }
}

fn audio_clip(id: ClipId, track: TrackId, start_tick: u32, length_ticks: u32, sample: SampleId) -> Clip {
    Clip {
        id,
        track,
        start_tick,
        length_ticks,
        content: ClipContent::audio(sample),
        gain: 1.0,
        fade_in_ticks: 0,
        fade_out_ticks: 0,
        fade_in_shape: Default::default(),
        fade_out_shape: Default::default(),
        offset_ticks: 0,
        offset_frames: 0,
        warp: None,
        pitch_semitones: 0.0,
    }
}

struct FakeClips {
    events: Events,
    clips: Mutex<Vec<Clip>>,
}

impl ClipControl for FakeClips {
    fn clips(&self) -> Vec<Clip> {
        self.clips.lock().unwrap().clone()
    }
    fn loop_region(&self) -> Option<(u32, u32)> {
        None
    }
    fn add_note_clip(&self, _: NewNoteClip) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn add_note_clips(&self, _: Vec<NewNoteClip>) -> Result<Vec<Clip>, ContractError> {
        Err(unused())
    }
    /// Like the real one: a clip for the whole sound (always 1920 ticks here) and a refusal if the place is taken.
    fn add_audio_clip(&self, track: TrackId, start_tick: u32, sample: SampleId) -> Result<Clip, ContractError> {
        let clip = audio_clip(100 + sample, track, start_tick, 1920, sample);
        let busy = self.clips().iter().any(|c| c.track == track && c.overlaps(clip.start_tick.into(), clip.end_tick()));
        if busy {
            return Err(general::INTERNAL.with("details", "the place on the track is taken"));
        }
        self.events.lock().unwrap().push("clip.add".into());
        self.clips.lock().unwrap().push(clip.clone());
        Ok(clip)
    }
    fn edit_clip(&self, _: ClipId, _: ClipEdit) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn split_clip(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn trim_start(&self, _: ClipId, _: u32) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn duplicate_clip(&self, _: ClipId) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn remove_clip(&self, _: ClipId) -> Result<(), ContractError> {
        Ok(())
    }
    fn edit_clips(&self, _: Vec<(ClipId, ClipEdit)>) -> Result<Vec<Clip>, ContractError> {
        Ok(Vec::new())
    }
    fn remove_clips(&self, _: Vec<ClipId>) -> Result<(), ContractError> {
        Ok(())
    }
    fn copy_clips(&self, _: Vec<ClipId>, _: u32) -> Result<Vec<Clip>, ContractError> {
        Ok(Vec::new())
    }
    fn set_loop_region(&self, _: Option<(u32, u32)>) -> Result<(), ContractError> {
        Ok(())
    }
}

/// Takes: like the real one, the new sound becomes active, earlier ones stay in the list and the clip does not move.
struct FakeTakes {
    events: Events,
    clips: Arc<FakeClips>,
}

impl TakeControl for FakeTakes {
    fn add_take(&self, clip: ClipId, sample: SampleId) -> Result<Clip, ContractError> {
        self.events.lock().unwrap().push(format!("take.add {clip}"));
        let mut clips = self.clips.clips.lock().unwrap();
        let target = clips.iter_mut().find(|c| c.id == clip).ok_or_else(|| clips::NOT_FOUND.with("id", clip))?;
        let ClipContent::Audio { sample: old, takes, .. } = &target.content else { return Err(unused()) };
        let mut takes = if takes.is_empty() { vec![*old] } else { takes.clone() };
        takes.push(sample);
        target.content = ClipContent::Audio { sample, takes, comp: Vec::new() };
        Ok(target.clone())
    }
    fn set_active_take(&self, clip: ClipId, take: SampleId) -> Result<Clip, ContractError> {
        self.events.lock().unwrap().push(format!("take.active {clip} {take}"));
        let mut clips = self.clips.clips.lock().unwrap();
        let target = clips.iter_mut().find(|c| c.id == clip).ok_or_else(|| clips::NOT_FOUND.with("id", clip))?;
        if let ClipContent::Audio { sample, .. } = &mut target.content {
            *sample = take;
        }
        Ok(target.clone())
    }
    fn remove_take(&self, _: ClipId, _: SampleId) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn set_comp(&self, _: ClipId, _: Vec<CompSegment>) -> Result<Clip, ContractError> {
        Err(unused())
    }
    fn choose_take_range(&self, _: ClipId, _: u32, _: u32, _: SampleId) -> Result<Clip, ContractError> {
        Err(unused())
    }
}

struct FakeSamples {
    events: Events,
    imported: Mutex<Vec<PathBuf>>,
}

impl SampleCatalog for FakeSamples {
    /// Samples get ids 7, 8, 9… in import order.
    fn import(&self, path: &Path) -> Result<SampleInfo, ContractError> {
        self.events.lock().unwrap().push("sample.import".into());
        let mut imported = self.imported.lock().unwrap();
        imported.push(path.to_path_buf());
        let id = 6 + imported.len() as SampleId;
        Ok(SampleInfo { id, name: "Recording".into(), path: path.to_path_buf(), frame_count: 0, sample_rate: 48_000, markers: Vec::new() })
    }
    fn samples(&self) -> Vec<SampleInfo> {
        Vec::new()
    }
    fn remove(&self, _: SampleId) -> Result<(), ContractError> {
        Ok(())
    }
}

struct FakeWriter {
    events: Events,
    written: Mutex<Vec<(PathBuf, SampleData)>>,
}

impl SampleWriter for FakeWriter {
    fn write(&self, path: &Path, data: &SampleData) -> Result<(), ContractError> {
        self.events.lock().unwrap().push("wav.write".into());
        self.written.lock().unwrap().push((path.to_path_buf(), data.clone()));
        Ok(())
    }
}

struct FakeAssets;

impl AssetStore for FakeAssets {
    fn adopt(&self, _: &Path, source: &Path) -> Result<PathBuf, ContractError> {
        Ok(source.to_path_buf())
    }
    fn new_asset_path(&self, project_file: &Path, name: &str) -> PathBuf {
        project_file.parent().unwrap().join("samples").join(format!("{name}.wav"))
    }
}

struct FakeProject {
    path: Mutex<Option<PathBuf>>,
}

impl ProjectIo for FakeProject {
    fn new_project(&self) {}
    fn save(&self, _: &Path) -> Result<(), ContractError> {
        Ok(())
    }
    fn load(&self, _: &Path) -> Result<(), ContractError> {
        Ok(())
    }
    fn create(&self, _: &Path) -> Result<(), ContractError> {
        Ok(())
    }
    fn restore_from(&self, _: &Path) -> Result<(), ContractError> {
        Ok(())
    }
    fn folder(&self) -> Option<PathBuf> {
        None
    }
    fn project_name(&self) -> String {
        "test".into()
    }
    fn path(&self) -> Option<PathBuf> {
        self.path.lock().unwrap().clone()
    }
    fn is_dirty(&self) -> bool {
        false
    }
    fn take_warnings(&self) -> Vec<ContractError> {
        Vec::new()
    }
}

struct FakeHistory {
    events: Events,
}

impl HistoryControl for FakeHistory {
    fn undo(&self) -> bool {
        false
    }
    fn redo(&self) -> bool {
        false
    }
    fn can_undo(&self) -> bool {
        false
    }
    fn can_redo(&self) -> bool {
        false
    }
    fn undo_label(&self) -> Option<String> {
        None
    }
    fn begin_group(&self, label: &str) {
        self.events.lock().unwrap().push(format!("group.begin {label}"));
    }
    fn end_group(&self) -> bool {
        self.events.lock().unwrap().push("group.end".into());
        true
    }
    fn cancel_group(&self) {}
    fn mark_baseline(&self) {}
    fn checkpoint(&self) -> HistoryMark {
        0
    }
    fn squash(&self, _: HistoryMark, _: &str) -> Result<bool, ContractError> {
        Ok(false)
    }
}

#[derive(Default)]
struct FakeLog(Mutex<Vec<LogEntry>>);

impl AppLog for FakeLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}

struct World {
    events: Events,
    stats: Arc<FakeStats>,
    capture: Arc<FakeCapture>,
    transport: Arc<FakeTransport>,
    clips: Arc<FakeClips>,
    samples: Arc<FakeSamples>,
    writer: Arc<FakeWriter>,
    devices: Arc<FakeDevices>,
    monitor: Arc<FakeMonitor>,
    project: Arc<FakeProject>,
    log: Arc<FakeLog>,
    recorder: AudioRecorder,
}

fn world_with(max_samples: usize) -> World {
    let events = Events::default();
    let capture = Arc::new(FakeCapture {
        events: events.clone(),
        pending: Mutex::default(),
        fail_start: Mutex::default(),
        dropped: AtomicU64::new(0),
    });
    // A 10 ms latency = 480 frames at 48 kHz: a round number that makes the cut easy to check.
    let devices = Arc::new(FakeDevices {
        settings: Mutex::new(AudioSettings { record_latency_ms: Some(10.0), ..AudioSettings::default() }),
        level: Mutex::default(),
    });
    let transport = Arc::new(FakeTransport {
        events: events.clone(),
        state: Mutex::new(TransportState::Stopped),
        position: AtomicU64::new(960),
        holds: Mutex::new(0),
        loop_region: Mutex::new(None),
    });
    let clips = Arc::new(FakeClips { events: events.clone(), clips: Mutex::default() });
    let samples = Arc::new(FakeSamples { events: events.clone(), imported: Mutex::default() });
    let writer = Arc::new(FakeWriter { events: events.clone(), written: Mutex::default() });
    let project = Arc::new(FakeProject { path: Mutex::new(Some(PathBuf::from("/p/song.dawproj"))) });
    let log = Arc::new(FakeLog::default());
    let stats = Arc::new(FakeStats::default());
    let monitor = Arc::new(FakeMonitor::default());
    let recorder = AudioRecorder::new(Deps {
        capture: capture.clone(),
        devices: devices.clone(),
        stats: stats.clone(),
        monitor: monitor.clone(),
        transport: transport.clone(),
        tempo: Arc::new(FakeTempo),
        takes: Arc::new(FakeTakes { events: events.clone(), clips: clips.clone() }),
        clips: clips.clone(),
        tracks: Arc::new(FakeTracks),
        samples: samples.clone(),
        writer: writer.clone(),
        assets: Arc::new(FakeAssets),
        project: project.clone(),
        history: Arc::new(FakeHistory { events: events.clone() }),
        log: log.clone(),
        max_samples,
    });
    World { events, stats, capture, transport, clips, samples, writer, devices, monitor, project, log, recorder }
}

fn world() -> World {
    world_with(Deps::DEFAULT_MAX_SAMPLES)
}

impl World {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    /// Arms the track and starts a take.
    fn record(&self, track: TrackId) {
        self.recorder.set_armed(track, true).unwrap();
        self.recorder.start().unwrap();
    }

    /// Stops a one-track take and returns its clip.
    fn stop_one(&self) -> Clip {
        let mut clips = self.recorder.stop().unwrap();
        assert_eq!(clips.len(), 1);
        clips.remove(0)
    }
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

/// `frames` stereo frames whose value equals their index (shows how much was cut).
fn ramp(frames: usize) -> Vec<f32> {
    (0..frames).flat_map(|i| [i as f32, i as f32]).collect()
}

/// Error of a fake method this test never calls.
fn unused() -> ContractError {
    general::INTERNAL.with("details", "not used by this test")
}
