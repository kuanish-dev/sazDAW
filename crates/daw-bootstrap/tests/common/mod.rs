//! Shared helpers for full-session tests without an audio device: building, rendering blocks, levels, WAV files, recording notes.
#![allow(dead_code, reason = "each test file is its own binary and takes only the helpers it needs")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test helpers: a failed setup should stop the test loudly, and skips say why"
)]

pub use std::path::{Path, PathBuf};

pub use daw_bootstrap::{Session, SessionConfig, build_headless_session_in, build_session};
pub use daw_contracts::*;

pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daw-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The value of an error param (`None` if the error has no such param).
pub fn param<'a>(error: &'a ContractError, name: &str) -> Option<&'a str> {
    error.params().iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
}

/// A graph bus by name (in the demo project: "Drums", "Bass", "Reverb").
pub fn bus_named(controls: &Controls, name: &str) -> NodeId {
    let graph = controls.graph.graph();
    graph.nodes.iter().find(|n| n.name == name && matches!(n.kind, NodeKind::Bus { .. })).map(|n| n.id).expect("the bus exists")
}

/// The source node of a track (created together with the track).
pub fn source_node(controls: &Controls, track: TrackId) -> NodeId {
    controls.graph.graph().source_of(track).map(|n| n.id).expect("the track has a source node")
}

pub fn add_synth_track(controls: &Controls, name: &str) -> TrackId {
    controls.tracks.add_track(name, Some(SYNTH_KIND), 60).expect("the built-in synth is in the catalog")
}

/// Kind of the dev-only fixture synth plugin (`test-plugins/synth`) once a scan has found it.
pub const FIXTURE_SYNTH: &str = "vst3:synth";

/// A headless session whose plugin scan sees the dev-only fixture plugins (`test-plugins/*/target/bundled`, built by
/// `scripts/build-test-plugins.sh`; loaded in this process), already scanned. `None` (with a message) when the fixture
/// synth is not built: the plugin tests are then skipped.
pub fn start_with_fixture_plugins(name: &str) -> Option<Session> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-plugins");
    let dirs: Vec<PathBuf> = ["synth", "distortion"].iter().map(|p| Path::new(root).join(p).join("target/bundled")).collect();
    if !dirs[0].join("Synth.vst3").exists() {
        eprintln!("SKIPPED: no fixture Synth.vst3 in {} (build it: scripts/build-test-plugins.sh)", dirs[0].display());
        return None;
    }
    let config = SessionConfig { state_dir: temp_dir(&format!("fixtures-{name}")), plugin_dirs: Some(dirs) };
    let mut session = build_headless_session_in(&config);
    session.renderer.prepare(StreamConfig { sample_rate: 48_000, channels: 2 });
    session.controls.plugins.scan().expect("the fixture scan runs");
    Some(session)
}

/// A note clip `bars` bars long, starting at bar `start_bar`.
pub fn note_clip(controls: &Controls, track: TrackId, start_bar: u32, bars: u32) -> Clip {
    controls.clips.add_note_clip(NewNoteClip::empty(track, start_bar * BAR_TICKS, Some(bars * BAR_TICKS))).unwrap()
}

/// A synth track with a one-bar clip at the start and one note in it.
pub fn synth_track_with_note(controls: &Controls, new_note: NewNote) -> (TrackId, ClipId) {
    let track = add_synth_track(controls, "Synth");
    let clip = note_clip(controls, track, 0, 1);
    controls.notes.add_note(clip.id, new_note).unwrap();
    (track, clip.id)
}

/// The track clips in timeline order.
pub fn track_clips(controls: &Controls, track: TrackId) -> Vec<Clip> {
    let mut clips: Vec<_> = controls.clips.clips().into_iter().filter(|c| c.track == track).collect();
    clips.sort_by_key(|c| c.start_tick);
    clips
}

pub fn start(demo: bool) -> Session {
    let mut session = build_session(demo);
    session.renderer.prepare(StreamConfig { sample_rate: 48_000, channels: 2 });
    session
}

pub fn render(session: &mut Session, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    for chunk in out.chunks_mut(2048) {
        session.renderer.render(chunk);
    }
    out
}

pub fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
}

pub fn rms(buffer: &[f32]) -> f32 {
    (buffer.iter().map(|v| v * v).sum::<f32>() / buffer.len().max(1) as f32).sqrt()
}

pub fn note(pitch: u8, start: u32, length: u32) -> NewNote {
    NewNote { pitch, start_tick: start, length_ticks: length, velocity: 110 }
}

pub fn write_wav(path: &Path, seconds: f32, hz: f32) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(44_100.0 * seconds) as usize {
        let v = (std::f32::consts::TAU * hz * i as f32 / 44_100.0).sin() * 0.6;
        writer.write_sample((v * 32_767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
}

pub fn read_wav(path: &Path) -> (hound::WavSpec, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap() as f32 / scale).collect()
        }
    };
    (spec, samples)
}

pub fn wait_for(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// Plays "live" with recording armed: a 100-tick pause, then a 480-tick note (120 BPM → 25 frames per tick).
pub fn play_one_note(session: &mut Session, track: TrackId) {
    session.controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    session.controls.record.set_armed(true);
    session.controls.transport.play();
    render(session, 2_500);
    session.controls.live_notes.note_on(track, 60, 1.0);
    render(session, 12_000);
    session.controls.live_notes.note_off(track, 60);
    render(session, 512);
    session.controls.transport.stop();
    render(session, 512);
}

/// All notes in all clips of the track.
pub fn recorded_notes(session: &Session, track: TrackId) -> Vec<Note> {
    track_clips(&session.controls, track).iter().flat_map(|clip| session.controls.notes.notes(clip.id).unwrap()).collect()
}

/// 120 BPM at 48 kHz: 25 frames per tick.
pub fn render_ticks(session: &mut Session, ticks: u32) {
    render(session, ticks as usize * 25);
}

pub fn start_recording(session: &mut Session) -> TrackId {
    let track = add_synth_track(&session.controls, "Synth");
    session.controls.record.set_quantize(0).unwrap();
    session.controls.tempo.set_tempo(Bpm::new(120.0).unwrap()).unwrap();
    session.controls.record.set_armed(true);
    session.controls.transport.play();
    track
}

pub fn tap(session: &mut Session, track: TrackId, pitch: u8, hold_ticks: u32) {
    session.controls.live_notes.note_on(track, pitch, 1.0);
    render_ticks(session, hold_ticks);
    session.controls.live_notes.note_off(track, pitch);
    render_ticks(session, 4);
}

pub fn finish_take(session: &mut Session, expected: usize) {
    session.controls.transport.stop();
    render(session, 512);
    // The take is finished when the undo group is closed: by then all events are processed.
    assert!(wait_for(|| session.controls.history.undo_label().as_deref() == Some("Recording")), "the take is closed");
    assert_eq!(session.controls.record.status().take_notes, expected, "notes in the take");
    session.controls.record.set_armed(false);
}

/// A 1 s file at 44.1 kHz: 0–0.4 s a 440 Hz tone, a pause, 0.5–0.9 s an 880 Hz tone.
pub fn write_two_tones(path: &Path) {
    let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..44_100usize {
        let t = i as f32 / 44_100.0;
        let hz = if t < 0.4 {
            440.0
        } else if (0.5..0.9).contains(&t) {
            880.0
        } else {
            0.0
        };
        let v = if hz > 0.0 { (std::f32::consts::TAU * hz * t).sin() * 0.6 } else { 0.0 };
        writer.write_sample((v * 32_767.0) as i16).unwrap();
    }
    writer.finalize().unwrap();
}

pub fn zero_crossings(buffer: &[f32]) -> usize {
    let mono: Vec<f32> = buffer.chunks(2).map(|f| f[0]).collect();
    mono.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
}

pub fn sampler_track(name: &str, sample: u32, start: f32, end: f32) -> NewTrack {
    NewTrack {
        name: name.into(),
        kind: Some("sampler".into()),
        default_pitch: 60,
        params: Params::from([
            ("sample".to_string(), sample as f32),
            ("start".to_string(), start),
            ("end".to_string(), end),
            ("gain".to_string(), 1.0),
            ("attack".to_string(), 0.0005),
        ]),
    }
}

pub fn preview_request(slots: &[(&str, u32)], lanes: Vec<LaneAssignment>) -> DrumPartRequest {
    DrumPartRequest {
        start_bar: 0,
        slots: slots.iter().map(|(pattern, repeat)| PatternSlot { pattern: (*pattern).into(), repeat: *repeat }).collect(),
        replace: false,
        lanes,
    }
}
