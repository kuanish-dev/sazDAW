//! Processing tests: length, pitch, silence, NaN, edges; the cache against the `ProcessedAudio` contract.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use daw_contracts::codes::clips;
use daw_contracts::{
    AppLog, Bpm, Clip, ClipContent, LogEntry, ProcessJob, ProcessedAudio, SampleData, SampleId, SampleStore, SignatureChange,
    StretchAnchor, TempoChange, TempoMap, TimeSignature, Warp,
};

use crate::{StretchCache, process, process_curve, process_job};

const RATE: u32 = 48_000;

fn sine(freq: f64, seconds: f64, channels: u16) -> SampleData {
    let len = (f64::from(RATE) * seconds) as usize;
    let frames = (0..len)
        .flat_map(|i| {
            let v = (0.5 * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(RATE)).sin()) as f32;
            std::iter::repeat_n(v, usize::from(channels))
        })
        .collect();
    SampleData { sample_rate: RATE, channels, frames }
}

/// Frequency from zero crossings in the middle of the audio (window edges do not count).
fn measured_frequency(data: &SampleData) -> f64 {
    let channels = usize::from(data.channels);
    let mono: Vec<f32> = data.frames.chunks_exact(channels).map(|f| f[0]).collect();
    let (from, to) = (mono.len() / 5, mono.len() * 4 / 5);
    let crossings = mono[from..to].windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    crossings as f64 * f64::from(RATE) / (to - from) as f64
}

fn rms(data: &SampleData) -> f64 {
    let sum: f64 = data.frames.iter().map(|v| f64::from(*v).powi(2)).sum();
    (sum / data.frames.len().max(1) as f64).sqrt()
}

#[test]
fn length_is_exact_for_any_stretch() {
    let source = sine(220.0, 0.5, 2);
    for stretch in [0.25, 0.5, 0.8, 1.0, 1.25, 2.0, 4.0] {
        for semitones in [0.0, 5.0, -7.0] {
            let out = process(&source, stretch, semitones).unwrap();
            let expected = (source.frame_count() as f64 * stretch).round() as usize;
            assert_eq!(out.frame_count(), expected, "stretch {stretch}, pitch {semitones}");
            assert_eq!(out.channels, 2);
            assert_eq!(out.sample_rate, RATE);
        }
    }
}

#[test]
fn time_stretch_keeps_the_pitch_and_level() {
    let source = sine(440.0, 0.6, 1);
    for stretch in [0.75, 1.5] {
        let out = process(&source, stretch, 0.0).unwrap();
        let freq = measured_frequency(&out);
        assert!((freq - 440.0).abs() < 440.0 * 0.02, "stretch {stretch}: frequency {freq}");
        assert!((rms(&out) / rms(&source) - 1.0).abs() < 0.15, "the level is almost the same");
    }
}

#[test]
fn pitch_shift_moves_the_frequency_and_keeps_the_length() {
    let source = sine(440.0, 0.6, 2);
    for (semitones, expected) in [(12.0, 880.0), (-12.0, 220.0), (3.0, 440.0 * 2f64.powf(0.25)), (7.0, 440.0 * 2f64.powf(7.0 / 12.0))] {
        let out = process(&source, 1.0, semitones).unwrap();
        assert_eq!(out.frame_count(), source.frame_count());
        let freq = measured_frequency(&out);
        assert!((freq - expected).abs() < expected * 0.02, "{semitones} semitones: {freq} instead of {expected}");
    }
    let both = process(&source, 1.5, 12.0).unwrap();
    assert_eq!(both.frame_count(), (source.frame_count() as f64 * 1.5).round() as usize);
    assert!((measured_frequency(&both) - 880.0).abs() < 880.0 * 0.02);
}

#[test]
fn silence_stays_silence_and_nan_does_not_leak() {
    let silence = SampleData { sample_rate: RATE, channels: 2, frames: vec![0.0; 2 * 9000] };
    let out = process(&silence, 1.3, 4.0).unwrap();
    assert!(out.frames.iter().all(|v| *v == 0.0));
    let mut broken = sine(300.0, 0.2, 1);
    broken.frames[1000] = f32::NAN;
    broken.frames[2000] = f32::INFINITY;
    let out = process(&broken, 0.9, -2.0).unwrap();
    assert!(out.frames.iter().all(|v| v.is_finite()));
}

#[test]
fn identity_is_a_copy_and_tiny_or_empty_sounds_work() {
    let source = sine(100.0, 0.1, 2);
    assert_eq!(process(&source, 1.0, 0.0).unwrap(), source);
    let empty = SampleData { sample_rate: RATE, channels: 1, frames: Vec::new() };
    assert_eq!(process(&empty, 2.0, 3.0).unwrap().frame_count(), 0);
    let short = SampleData { sample_rate: RATE, channels: 1, frames: vec![0.5; 10] };
    assert_eq!(process(&short, 2.0, 0.0).unwrap().frame_count(), 20);
}

#[test]
fn limits_are_refused() {
    let source = sine(100.0, 0.05, 1);
    assert!(matches!(process(&source, 5.0, 0.0), Err(e) if e.is(clips::STRETCH_OUT_OF_RANGE)));
    assert!(matches!(process(&source, 0.1, 0.0), Err(e) if e.is(clips::STRETCH_OUT_OF_RANGE)));
    assert!(matches!(process(&source, 1.0, 25.0), Err(e) if e.is(clips::PITCH_OUT_OF_RANGE)));
    assert!(matches!(process(&source, 1.0, f32::NAN), Err(e) if e.is(clips::PITCH_OUT_OF_RANGE)));
}

#[test]
fn a_straight_curve_sounds_like_one_stretch() {
    let source = sine(330.0, 1.0, 2);
    let anchors = [StretchAnchor { source: 0, output: 0 }, StretchAnchor { source: 48_000, output: 72_000 }];
    assert_eq!(process_curve(&source, &anchors, 0.0).unwrap(), process(&source, 1.5, 0.0).unwrap());
    let pitched = process_curve(&source, &anchors, 5.0).unwrap();
    assert_eq!(pitched.frame_count(), 72_000);
    assert!((measured_frequency(&pitched) - 330.0 * 2f64.powf(5.0 / 12.0)).abs() < 330.0 * 0.03);
}

#[test]
fn bad_curves_are_refused() {
    let source = sine(100.0, 0.5, 1);
    let one = [StretchAnchor { source: 0, output: 0 }];
    assert!(matches!(process_curve(&source, &one, 0.0), Err(e) if e.is(clips::STRETCH_CURVE_TOO_SHORT)));
    let back = [StretchAnchor { source: 100, output: 0 }, StretchAnchor { source: 50, output: 100 }];
    assert!(matches!(process_curve(&source, &back, 0.0), Err(e) if e.is(clips::STRETCH_CURVE_UNORDERED)));
    let wild = [StretchAnchor { source: 0, output: 0 }, StretchAnchor { source: 100, output: 1000 }];
    assert!(matches!(process_curve(&source, &wild, 0.0), Err(e) if e.is(clips::STRETCH_OUT_OF_RANGE)));
}

/// "Clicks on every beat" at tempo 120 (0.5 s): short bursts of a 1 kHz sine centred on each beat.
fn clicks(beats: usize) -> SampleData {
    let beat = RATE as usize / 2;
    let mut frames = vec![0.0f32; beat * beats];
    for b in 0..beats {
        for i in 0..144 {
            let at = b * beat + i;
            let envelope = (std::f64::consts::PI * i as f64 / 144.0).sin();
            let value = envelope * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / f64::from(RATE)).sin();
            frames[at] = value as f32;
        }
    }
    SampleData { sample_rate: RATE, channels: 1, frames }
}

/// Where near frame `near` (±100 ms) the audio is loudest (the envelope is the sum of squares over 1 ms).
fn energy_centre(data: &SampleData, near: f64) -> f64 {
    let span = f64::from(RATE) * 0.1;
    let from = (near - span).max(0.0) as usize;
    let to = ((near + span) as usize).min(data.frames.len() - 48);
    let energy = |i: usize| data.frames[i..i + 48].iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    let loudest = (from..to).max_by(|a, b| energy(*a).total_cmp(&energy(*b))).expect("there are frames");
    assert!(energy(loudest) > 1e-6, "silence near frame {near}");
    loudest as f64
}

/// A fitted clip (source tempo 120) of `beats` beats from the song start under map `map`: every beat of the audio must
/// land where that beat is on the map. Tolerance is 5 ms plus 50 ms × (stretch - 1): WSOLA places the attack early even
/// with one stretch (by ~17 ms at 1.33: the 40 ms window picks the hit before its nominal place), while without the
/// curve beats after a tempo change would drift by hundreds of milliseconds.
fn assert_beats_follow(map: &TempoMap, beats: usize) {
    let clip = Clip {
        id: 1,
        track: 1,
        start_tick: 0,
        length_ticks: 960 * beats as u32,
        content: ClipContent::audio(1),
        gain: 1.0,
        fade_in_ticks: 0,
        fade_out_ticks: 0,
        fade_in_shape: Default::default(),
        fade_out_shape: Default::default(),
        offset_ticks: 0,
        offset_frames: 0,
        warp: Some(Warp { source_bpm: 120.0 }),
        pitch_semitones: 0.0,
    };
    let job = ProcessJob::for_clip(&clip, 1, map, RATE).expect("a job");
    assert!(job.curve().is_some(), "the tempo changes inside the clip, so a curve is needed");
    let source = clicks(beats);
    let out = process_job(&source, &job).unwrap();
    let expected_len = map.frames_between(0.0, f64::from(clip.length_ticks), RATE);
    assert!((out.frame_count() as f64 - expected_len).abs() <= 1.0, "the length follows the map");
    let source_offset = energy_centre(&source, 0.0);
    for beat in 1..beats {
        let expected = map.frames_between(0.0, 960.0 * beat as f64, RATE) + source_offset;
        let found = energy_centre(&out, expected);
        let error_ms = (found - expected).abs() / f64::from(RATE) * 1000.0;
        let stretch = 120.0 / map.tempo_at(960.0 * beat as f64).get();
        let allowed = 5.0 + 50.0 * (stretch - 1.0).abs();
        assert!(error_ms < allowed, "beat {beat}: {error_ms:.1} ms off the map (tolerance {allowed:.1})");
    }
}

fn map(changes: &[(u32, f64, bool)]) -> TempoMap {
    let tempos = changes.iter().map(|&(tick, bpm, ramp)| TempoChange { tick, bpm: Bpm::new(bpm).unwrap(), ramp }).collect();
    TempoMap::new(tempos, vec![SignatureChange { tick: 0, signature: TimeSignature::FOUR_FOUR }]).unwrap()
}

#[test]
fn warped_clip_stays_on_the_beat_across_a_tempo_change() {
    // One stretch by the starting tempo would put beat 8 at 3.5 s instead of 4.0, half a second off.
    assert_beats_follow(&map(&[(0, 120.0, false), (3840, 90.0, false)]), 8);
}

#[test]
fn warped_clip_follows_a_tempo_ramp() {
    assert_beats_follow(&map(&[(0, 120.0, false), (7680, 80.0, true)]), 8);
}

#[derive(Default)]
struct Store(Mutex<HashMap<SampleId, Arc<SampleData>>>);

impl SampleStore for Store {
    fn get(&self, id: SampleId) -> Option<Arc<SampleData>> {
        self.0.lock().unwrap().get(&id).cloned()
    }
    fn insert(&self, id: SampleId, data: Arc<SampleData>) {
        self.0.lock().unwrap().insert(id, data);
    }
    fn remove(&self, id: SampleId) {
        self.0.lock().unwrap().remove(&id);
    }
}

#[derive(Default)]
struct Log(Mutex<Vec<LogEntry>>);

impl AppLog for Log {
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

fn cache() -> (StretchCache, Arc<Log>) {
    let store = Arc::new(Store::default());
    store.insert(1, Arc::new(sine(200.0, 0.2, 1)));
    let log = Arc::new(Log::default());
    (StretchCache::new(store, log.clone()), log)
}

fn wait_finished(cache: &StretchCache, count: u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while cache.finished() < count {
        assert!(Instant::now() < deadline, "the background job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn ready_starts_a_background_job_and_then_answers() {
    let (cache, log) = cache();
    let job = ProcessJob::new(1, 1.5, 0.0);
    assert!(cache.ready(&job).is_none(), "not computed yet the first time");
    wait_finished(&cache, 1);
    let data = cache.ready(&job).expect("ready");
    assert_eq!(data.frame_count(), 14_400);
    assert!(Arc::ptr_eq(&data, &cache.ready(&job).unwrap()), "the second time the same audio comes from the cache");
    assert!(log.0.lock().unwrap().is_empty());
}

#[test]
fn failures_are_logged_once_and_counted() {
    let (cache, log) = cache();
    let missing = ProcessJob::new(9, 1.2, 0.0);
    assert!(cache.ready(&missing).is_none());
    wait_finished(&cache, 1);
    assert!(cache.ready(&missing).is_none(), "a failure is not retried");
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(cache.finished(), 1);
    let entries = log.0.lock().unwrap().clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "stretch");
    assert!(matches!(cache.compute(&ProcessJob::new(1, 9.0, 0.0)), Err(e) if e.is(clips::STRETCH_OUT_OF_RANGE)));
}

#[test]
fn a_job_dropped_while_queued_and_wanted_again_is_still_computed() {
    let (cache, log) = cache();
    let first = ProcessJob::new(1, 1.5, 0.0);
    let dropped = ProcessJob::new(1, 0.8, 0.0);
    assert!(cache.ready(&first).is_none());
    assert!(cache.ready(&dropped).is_none());
    cache.retain(std::slice::from_ref(&first));
    assert!(cache.ready(&dropped).is_none(), "wanted again while it may still be queued");
    // The project asks again only when `finished` grows, so the job must finish without another request.
    wait_finished(&cache, 2);
    assert!(cache.ready(&dropped).is_some());
    assert!(log.0.lock().unwrap().is_empty());
}

#[test]
fn retain_evicts_unused_sounds_and_compute_waits() {
    let (cache, _) = cache();
    let a = ProcessJob::new(1, 0.5, 0.0);
    let b = ProcessJob::new(1, 1.0, 2.0);
    let data = cache.compute(&a).unwrap();
    assert_eq!(data.frame_count(), 4800);
    assert!(Arc::ptr_eq(&data, &cache.compute(&a).unwrap()));
    cache.compute(&b).unwrap();
    cache.retain(std::slice::from_ref(&b));
    assert!(cache.ready(&b).is_some(), "wanted audio stays");
    cache.retain(&[]);
    assert!(cache.ready(&a).is_none(), "unwanted audio is dropped and computed again");
}
