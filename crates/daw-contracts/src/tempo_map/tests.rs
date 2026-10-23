use super::*;
use crate::ErrorKind;

fn bpm(value: f64) -> Bpm {
    Bpm::new(value).unwrap()
}

fn tempo(tick: u32, value: f64, ramp: bool) -> TempoChange {
    TempoChange { tick, bpm: bpm(value), ramp }
}

fn signature(tick: u32, numerator: u8, denominator: u8) -> SignatureChange {
    SignatureChange { tick, signature: TimeSignature::new(numerator, denominator).unwrap() }
}

fn four_four() -> Vec<SignatureChange> {
    vec![signature(0, 4, 4)]
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn a_constant_map_converts_like_the_tempo() {
    let map = TempoMap::constant(bpm(120.0), TimeSignature::FOUR_FOUR);
    close(map.seconds_at(1920.0), 1.0);
    close(map.tick_at_seconds(2.5), 4800.0);
    close(map.frames_between(0.0, 960.0, 48_000), 24_000.0);
    close(map.seconds_at(-960.0), -0.5);
    assert_eq!(map.tempo_at(100_000.0), bpm(120.0));
    assert!(!map.has_changes());
}

#[test]
fn a_step_changes_the_tempo_exactly_at_its_tick() {
    let map = TempoMap::new(vec![tempo(0, 120.0, false), tempo(3840, 60.0, false)], four_four()).unwrap();
    close(map.seconds_at(3840.0), 2.0);
    close(map.seconds_at(7680.0), 6.0);
    close(map.tick_at_seconds(4.0), 3840.0 + 1920.0);
    assert_eq!(map.tempo_at(3839.0), bpm(120.0));
    assert_eq!(map.tempo_at(3840.0), bpm(60.0));
    assert_eq!(map.next_change(0.0), Some(3840.0));
    assert_eq!(map.next_change(3840.0), None);
}

#[test]
fn a_ramp_is_integrated_in_closed_form() {
    let map = TempoMap::new(vec![tempo(0, 60.0, false), tempo(3840, 120.0, true)], four_four()).unwrap();
    close(map.seconds_at(3840.0), 4.0 * 2f64.ln());
    close(map.tempo_at(1920.0).get(), 90.0);
    assert!(map.in_ramp(100.0));
    assert!(!map.in_ramp(3840.0));
    let steps = 100_000;
    let numeric: f64 = (0..steps)
        .map(|i| {
            let tick = (f64::from(i) + 0.5) * 3840.0 / f64::from(steps);
            3840.0 / f64::from(steps) * 60.0 / (f64::from(PPQ) * map.tempo_at(tick).get())
        })
        .sum();
    close(map.seconds_at(3840.0), numeric);
    close(map.seconds_at(3840.0 + 1920.0) - map.seconds_at(3840.0), 1.0);
}

#[test]
fn seconds_and_ticks_round_trip_through_ramps_and_steps() {
    let map = TempoMap::new(
        vec![tempo(0, 100.0, false), tempo(7680, 140.0, true), tempo(11_520, 80.0, false), tempo(15_360, 160.0, true)],
        four_four(),
    )
    .unwrap();
    for tick in [0.0, 1.0, 3000.0, 7680.0, 9000.5, 11_520.0, 13_000.0, 15_360.0, 40_000.0, -500.0] {
        close(map.tick_at_seconds(map.seconds_at(tick)), tick);
    }
    let after = map.tick_after_frames(9000.0, 12_345.0, 48_000);
    close(map.frames_between(9000.0, after, 48_000), 12_345.0);
}

#[test]
fn bars_follow_signature_changes() {
    let map = TempoMap::new(vec![tempo(0, 120.0, false)], vec![signature(0, 3, 4), signature(5760, 4, 4)]).unwrap();
    assert_eq!(map.bar_tick(0), 0);
    assert_eq!(map.bar_tick(1), 2880);
    assert_eq!(map.bar_tick(2), 5760);
    assert_eq!(map.bar_tick(3), 9600);
    assert_eq!(map.bar_index(2879.0), 0);
    assert_eq!(map.bar_index(2880.0), 1);
    assert_eq!(map.bar_index(9599.0), 2);
    assert_eq!(map.bar_index(9600.0), 3);
    close(map.bar_start(7000.0), 5760.0);
    close(map.bar_start(3000.0), 2880.0);
    assert_eq!(map.bar_length_at(5759), 2880);
    assert_eq!(map.bar_length_at(5760), 3840);
    assert_eq!(map.signature_at(6000.0).signature, TimeSignature::FOUR_FOUR);
    assert_eq!(map.next_change(100.0), Some(5760.0));
}

#[test]
fn invalid_maps_are_refused() {
    let no_base = TempoMap::new(vec![tempo(960, 120.0, false)], four_four());
    assert!(matches!(no_base, Err(e) if e.kind() == ErrorKind::Invalid));
    let unordered = TempoMap::new(vec![tempo(0, 120.0, false), tempo(960, 90.0, false), tempo(960, 80.0, false)], four_four());
    assert!(matches!(unordered, Err(e) if e.kind() == ErrorKind::Invalid));
    let mid_bar = TempoMap::new(vec![tempo(0, 120.0, false)], vec![signature(0, 4, 4), signature(960, 3, 4)]);
    assert!(matches!(mid_bar, Err(e) if e.kind() == ErrorKind::Invalid));
    assert!(TempoMap::new(vec![tempo(0, 120.0, false)], vec![]).is_err());
    let too_many = (0..=MAX_TEMPO_CHANGES as u32).map(|i| tempo(i, 120.0, false)).collect();
    assert!(matches!(TempoMap::new(too_many, four_four()), Err(e) if e.is(codes::TEMPO_MAP_TOO_MANY)));
}

#[test]
fn the_first_point_never_ramps() {
    let map = TempoMap::new(vec![tempo(0, 120.0, true)], four_four()).unwrap();
    assert!(!map.tempos()[0].ramp);
}

#[test]
fn json_round_trips_and_is_checked() {
    let map = TempoMap::new(vec![tempo(0, 120.0, false), tempo(3840, 140.0, true)], vec![signature(0, 7, 8)]).unwrap();
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(
        json,
        r#"{"tempos":[{"tick":0,"bpm":120.0,"ramp":false},{"tick":3840,"bpm":140.0,"ramp":true}],"signatures":[{"tick":0,"signature":{"numerator":7,"denominator":8}}]}"#
    );
    assert_eq!(serde_json::from_str::<TempoMap>(&json).unwrap(), map);
    assert!(serde_json::from_str::<TempoMap>(r#"{"tempos":[],"signatures":[]}"#).is_err());
}
