use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct MemoryStore(Mutex<HashMap<u32, Arc<SampleData>>>);

impl SampleStore for MemoryStore {
    fn get(&self, id: u32) -> Option<Arc<SampleData>> {
        self.0.lock().unwrap().get(&id).cloned()
    }
    fn insert(&self, id: u32, data: Arc<SampleData>) {
        self.0.lock().unwrap().insert(id, data);
    }
    fn remove(&self, id: u32) {
        self.0.lock().unwrap().remove(&id);
    }
}

fn ramp(len: usize) -> Arc<SampleData> {
    // A ramp: the value grows linearly, which makes the playback speed easy to check.
    let frames = (0..len).map(|i| i as f32 / len as f32).collect();
    Arc::new(SampleData { sample_rate: 48_000, channels: 1, frames })
}

fn make(params: &[(&str, f32)], data: Option<Arc<SampleData>>) -> Box<dyn Instrument> {
    let store = Arc::new(MemoryStore::default());
    let mut map: Params = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    if let Some(data) = data {
        store.insert(1, data);
        map.insert("sample".into(), 1.0);
    }
    let mut instrument = factory(store).create(&map, None).unwrap();
    instrument.prepare(StreamConfig { sample_rate: 48_000, channels: 2 });
    instrument
}

fn render(i: &mut dyn Instrument, frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0; frames * 2];
    i.render(&mut AudioBlock::new(&mut buffer, 2));
    buffer
}

#[test]
fn plays_the_sample_and_stops_at_its_end() {
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005)], Some(ramp(1000)));
    assert!(render(s.as_mut(), 64).iter().all(|v| *v == 0.0));
    s.note_on(60, 1.0);
    let out = render(s.as_mut(), 2000);
    assert!(out[..1000].iter().any(|v| *v > 0.1));
    assert!(out[1990 * 2..].iter().all(|v| *v == 0.0), "silence after the end of the sound");
}

#[test]
fn octave_up_plays_twice_as_fast() {
    let mut normal = make(&[("gain", 1.0), ("attack", 0.0005)], Some(ramp(4000)));
    let mut octave = make(&[("gain", 1.0), ("attack", 0.0005)], Some(ramp(4000)));
    normal.note_on(60, 1.0);
    octave.note_on(72, 1.0);
    let a = render(normal.as_mut(), 1000);
    let b = render(octave.as_mut(), 1000);
    // After 500 frames an octave up reaches the same place as the normal pitch after 1000 (allowing for the attack).
    let at = |buf: &[f32], frame: usize| buf[frame * 2];
    assert!((at(&b, 500) - at(&a, 999)).abs() < 0.02, "{} vs {}", at(&b, 500), at(&a, 999));
}

#[test]
fn silent_without_sample_and_voice_count_is_bounded() {
    let mut empty = make(&[], None);
    empty.note_on(60, 1.0);
    assert!(render(empty.as_mut(), 256).iter().all(|v| *v == 0.0));

    let mut s = make(&[("loop", 1.0)], Some(ramp(500)));
    for note in 30..80 {
        s.note_on(note, 0.8);
    }
    let out = render(s.as_mut(), 4096);
    assert!(out.iter().all(|v| v.is_finite()));
}

#[test]
fn a_sample_missing_from_the_store_is_not_found() {
    let params: Params = [("sample".to_string(), 7.0)].into_iter().collect();
    let created = factory(Arc::new(MemoryStore::default())).create(&params, None);
    assert!(matches!(created, Err(e) if e.is(samples::NOT_FOUND)));
}

#[test]
fn looping_voice_releases_on_all_notes_off() {
    let mut s = make(&[("loop", 1.0), ("one_shot", 0.0), ("release", 0.01)], Some(ramp(300)));
    s.note_on(60, 1.0);
    assert!(render(s.as_mut(), 5000).iter().any(|v| *v != 0.0));
    s.all_notes_off();
    render(s.as_mut(), 1000);
    assert!(render(s.as_mut(), 256).iter().all(|v| *v == 0.0));
}

fn left(buffer: &[f32], frame: usize) -> f32 {
    buffer[frame * 2]
}

#[test]
fn plays_only_the_selected_region() {
    // A 0..1 ramp over 1000 frames; the region 0.5..0.75 gives values from 0.5 to 0.75 and silence after 250 frames.
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005), ("start", 0.5), ("end", 0.75)], Some(ramp(1000)));
    s.note_on(60, 1.0);
    let out = render(s.as_mut(), 600);
    // After 40 frames the attack (24 frames) is over: the value is ≈ 0.5 + 40/1000.
    assert!((left(&out, 40) - 0.54).abs() < 0.02, "region start: {}", left(&out, 40));
    assert!(left(&out, 240) > 0.7 && left(&out, 240) < 0.76, "region end: {}", left(&out, 240));
    assert!(out[270 * 2..].iter().all(|v| *v == 0.0), "silence after the region");
}

#[test]
fn reverse_plays_the_region_backwards() {
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005), ("reverse", 1.0)], Some(ramp(1000)));
    s.note_on(60, 1.0);
    let out = render(s.as_mut(), 1200);
    assert!(left(&out, 40) > 0.93, "starts from the end: {}", left(&out, 40));
    assert!(left(&out, 500) < 0.55 && left(&out, 500) > 0.45, "in the middle: {}", left(&out, 500));
    assert!(left(&out, 100) > left(&out, 400), "values decrease");
    assert!(out[1005 * 2..].iter().all(|v| *v == 0.0), "silence past the file start");
}

#[test]
fn reversed_region_with_swapped_bounds_still_works() {
    // start > end means the region between them.
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005), ("start", 0.75), ("end", 0.5)], Some(ramp(1000)));
    s.note_on(60, 1.0);
    let out = render(s.as_mut(), 600);
    assert!((left(&out, 40) - 0.54).abs() < 0.02, "{}", left(&out, 40));
    assert!(out[270 * 2..].iter().all(|v| *v == 0.0));
}

#[test]
fn loop_wraps_inside_the_region_only() {
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005), ("loop", 1.0), ("start", 0.2), ("end", 0.3)], Some(ramp(1000)));
    s.note_on(60, 1.0);
    let out = render(s.as_mut(), 5000);
    // The first frames are the attack ramp; after that the sound stays strictly inside the region 0.2..0.3.
    assert!(out.iter().skip(100).step_by(2).all(|v| *v > 0.15 && *v < 0.35), "the sound stays inside the region");
    assert!(left(&out, 4900) > 0.0, "keeps sounding");
}

#[test]
fn degenerate_regions_never_break_playback() {
    for (start, end) in [(0.5, 0.5), (1.0, 1.0), (0.0, 0.0)] {
        let mut s = make(&[("gain", 1.0), ("start", start), ("end", end)], Some(ramp(500)));
        s.note_on(60, 1.0);
        let out = render(s.as_mut(), 300);
        assert!(out.iter().all(|v| v.is_finite()), "region {start}..{end}");
    }
}

/// A mono output used to play only the left channel of a stereo sample.
#[test]
fn mono_output_gets_both_channels() {
    let stereo = Arc::new(SampleData { sample_rate: 48_000, channels: 2, frames: [0.0, 1.0].repeat(1000) });
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005)], Some(stereo));
    s.note_on(60, 1.0);
    let mut mono = vec![0.0; 200];
    s.render(&mut AudioBlock::new(&mut mono, 1));
    assert!((mono[100] - 0.5).abs() < 0.01, "midway between left (0) and right (1): {}", mono[100]);
}

#[test]
fn defaults_come_from_the_parameter_table() {
    let settings = Settings::from_params(&Params::new());
    assert_eq!((settings.end, settings.root, settings.gain, settings.one_shot), (1.0, 60.0, 0.8, true));
}

#[test]
fn factory_rebuilds_on_sample_change() {
    let f = factory(Arc::new(MemoryStore::default()));
    assert_eq!(f.rebuild_on(), ["sample"]);
    assert_eq!(f.kind(), "sampler");
}

#[test]
fn full_pitch_bend_up_plays_two_semitones_higher() {
    let mut bent = make(&[("gain", 1.0), ("attack", 0.0005)], Some(ramp(4000)));
    let mut higher = make(&[("gain", 1.0), ("attack", 0.0005)], Some(ramp(4000)));
    bent.controller(ControllerKind::PitchBend, 1.0);
    bent.note_on(60, 1.0);
    higher.note_on(62, 1.0);
    let a = render(bent.as_mut(), 1000);
    let b = render(higher.as_mut(), 1000);
    assert!((a[999 * 2] - b[999 * 2]).abs() < 1e-3, "{} vs {}", a[999 * 2], b[999 * 2]);
    // The wheel centred gives the original speed.
    bent.controller(ControllerKind::PitchBend, 0.0);
    bent.all_notes_off();
}

#[test]
fn sustain_pedal_holds_released_notes_until_it_is_lifted() {
    let mut s = make(&[("gain", 1.0), ("attack", 0.0005), ("release", 0.0005), ("loop", 1.0), ("one_shot", 0.0)], Some(ramp(500)));
    s.controller(ControllerKind::Cc { number: CC_SUSTAIN }, 1.0);
    s.note_on(60, 1.0);
    render(s.as_mut(), 100);
    s.note_off(60);
    let held = render(s.as_mut(), 400);
    assert!(held[300 * 2..].iter().any(|v| *v > 0.01), "the pedal holds the note");
    s.controller(ControllerKind::Cc { number: CC_SUSTAIN }, 0.0);
    let after = render(s.as_mut(), 400);
    assert!(after[200 * 2..].iter().all(|v| *v == 0.0), "the pedal is released and the note has faded");
}
