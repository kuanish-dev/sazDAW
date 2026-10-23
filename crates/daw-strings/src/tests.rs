//! The string model: notes in tune, a string rings and a released one stops, `ring`, `tone` and `drive` do what they
//! say, voices are reused and stolen; every kind sounds through the `Instrument` glue.

use daw_contracts::{AudioBlock, InstrumentFactory, Params, StreamConfig};

use crate::engine::{Body, Engine, Settings, VOICES};
use crate::factories;

const RATE: f32 = 48_000.0;
const GUITAR: Body = Body { modes: [(100.0, 3.0), (205.0, 5.0)], pick: 0.13 };
/// No body: the bare string, for measurements.
const DRY: Settings = Settings { ring: 4.0, tone: 0.5, body: 0.0, drive: 0.0, gain: 1.0 };

fn render(engine: &mut Engine, settings: &Settings, frames: usize) -> Vec<f32> {
    (0..frames).map(|_| engine.next_sample(settings)).collect()
}

fn pluck(note: u8, settings: &Settings, frames: usize) -> Vec<f32> {
    let mut engine = Engine::new(GUITAR);
    engine.note_on(note, 0.8, settings);
    render(&mut engine, settings, frames)
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt()
}

fn seconds(seconds: f32) -> usize {
    (seconds * RATE) as usize
}

/// The fundamental near `expected` Hz: the autocorrelation peak among lags of 0.8–1.25 periods, refined by a parabola
/// through the peak and its neighbours.
fn measured_frequency(samples: &[f32], expected: f32) -> f32 {
    let period = RATE / expected;
    let correlation = |lag: usize| samples.iter().zip(&samples[lag..]).map(|(a, b)| a * b).sum::<f32>();
    let lags = (period * 0.8) as usize..=(period * 1.25) as usize;
    let best = lags.max_by(|&a, &b| correlation(a).total_cmp(&correlation(b))).unwrap_or(0);
    let (left, middle, right) = (correlation(best - 1), correlation(best), correlation(best + 1));
    let shift = 0.5 * (left - right) / (left - 2.0 * middle + right);
    RATE / (best as f32 + shift)
}

#[test]
fn notes_are_in_tune_across_the_range() {
    for note in [40u8, 45, 57, 69, 76] {
        let expected = crate::engine::frequency(note);
        let sound = pluck(note, &DRY, seconds(0.5));
        let measured = measured_frequency(&sound[seconds(0.1)..seconds(0.3)], expected);
        let cents = 1200.0 * (measured / expected).log2();
        assert!(cents.abs() < 5.0, "note {note}: {measured:.2} Hz instead of {expected:.2} ({cents:.1} cents)");
    }
}

#[test]
fn a_held_string_rings_and_a_released_one_stops() {
    let held = pluck(45, &DRY, seconds(1.2));

    let mut engine = Engine::new(GUITAR);
    engine.note_on(45, 0.8, &DRY);
    let mut released = render(&mut engine, &DRY, seconds(0.1));
    engine.note_off(45);
    released.extend(render(&mut engine, &DRY, seconds(1.1)));

    let late = seconds(0.9)..seconds(1.2);
    assert!(rms(&held[late.clone()]) > 1e-3, "a held string still rings after a second");
    assert!(rms(&released[late]) < rms(&held[seconds(0.9)..]) * 0.001, "a released string is damped");
    assert_eq!(engine.active_voices(), 0, "the damped voice is free again");
    assert!(released.iter().all(|v| v.is_finite()));
}

#[test]
fn a_longer_ring_rings_longer() {
    let tail = |ring| rms(&pluck(45, &Settings { ring, ..DRY }, seconds(2.0))[seconds(1.5)..]);
    assert!(tail(8.0) > tail(2.0) * 2.0);
}

#[test]
fn a_brighter_tone_has_more_high_harmonics() {
    let bright = |tone| brightness(&pluck(45, &Settings { tone, ..DRY }, seconds(0.3))[seconds(0.05)..]);
    assert!(bright(1.0) > bright(0.0) * 1.5);
}

/// The share of energy in the sample-to-sample difference: it grows with the high harmonics.
fn brightness(sound: &[f32]) -> f32 {
    let difference: Vec<f32> = sound.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&difference) / rms(sound)
}

#[test]
fn drive_flattens_the_peaks_but_keeps_the_loudness() {
    let crest = |sound: &[f32]| sound.iter().fold(0.0f32, |a, v| a.max(v.abs())) / rms(sound);
    let at = |drive| pluck(45, &Settings { drive, ..DRY }, seconds(0.3))[seconds(0.05)..].to_vec();
    let (clean, crunch, fuzz) = (at(0.0), at(0.3), at(0.8));
    assert!(crest(&crunch) < crest(&clean) && crest(&fuzz) < crest(&clean) * 0.8, "the peaks flatten");
    for driven in [&crunch, &fuzz] {
        let ratio = rms(driven) / rms(&clean);
        assert!((0.6..1.4).contains(&ratio), "the loudness stays near the clean sound's: ×{ratio}");
        assert!(driven.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }
}

#[test]
fn a_harder_pluck_is_louder() {
    let peak = |velocity| {
        let mut engine = Engine::new(GUITAR);
        engine.note_on(45, velocity, &DRY);
        render(&mut engine, &DRY, seconds(0.05)).iter().fold(0.0f32, |a, v| a.max(v.abs()))
    };
    assert!(peak(1.0) > peak(0.3) * 2.0);
}

#[test]
fn the_same_note_reuses_its_string_and_extra_notes_steal_a_voice() {
    let mut engine = Engine::new(GUITAR);
    engine.note_on(45, 0.8, &DRY);
    render(&mut engine, &DRY, 100);
    engine.note_on(45, 0.8, &DRY);
    assert_eq!(engine.active_voices(), 1, "re-plucking a note plucks its own string again");

    for note in 40..60 {
        engine.note_on(note, 0.8, &DRY);
    }
    assert_eq!(engine.active_voices(), VOICES);
    let sound = render(&mut engine, &DRY, seconds(0.5));
    assert!(sound.iter().all(|v| v.is_finite() && v.abs() < 4.0));
}

#[test]
fn silent_until_plucked() {
    let mut engine = Engine::new(GUITAR);
    assert!(render(&mut engine, &Settings::default(), 1024).iter().all(|v| *v == 0.0));
}

/// One note of every kind through the `Instrument` glue: stereo, both channels equal.
fn play(factory: &dyn InstrumentFactory, params: &Params, note: u8) -> Vec<f32> {
    let mut instrument = factory.create(params, None).unwrap_or_else(|e| panic!("{}: {e}", factory.kind()));
    instrument.prepare(StreamConfig { sample_rate: RATE as u32, channels: 2 });
    instrument.note_on(note, 0.9);
    let mut buffer = vec![0.0; seconds(1.0) * 2];
    instrument.render(&mut AudioBlock::new(&mut buffer, 2));
    buffer
}

#[test]
fn every_kind_sounds_within_full_scale() {
    for factory in factories() {
        let note = match factory.kind() {
            "ukulele" => 67,
            "bass_guitar" => 33,
            _ => 52,
        };
        let sound = play(factory.as_ref(), &Params::new(), note);
        let peak = sound.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak > 0.01 && peak <= 1.0, "{}: peak {peak}", factory.kind());
        assert!(sound.chunks_exact(2).all(|frame| frame[0] == frame[1] && frame[0].is_finite()), "{}", factory.kind());
    }
}

#[test]
fn kinds_share_the_parameters_with_their_own_defaults_and_the_electric_ones_have_drive() {
    let kinds = factories();
    assert_eq!(kinds.iter().map(|f| f.kind()).collect::<Vec<_>>(), ["guitar", "electric_guitar", "ukulele", "bass_guitar"]);
    let params = |kind: &str| kinds.iter().find(|f| f.kind() == kind).map(|f| f.describe()).unwrap_or_default();
    let names = |kind: &str| params(kind).iter().map(|p| p.name).collect::<Vec<_>>();
    assert_eq!(names("guitar"), ["ring", "tone", "body", "gain"]);
    assert_eq!(names("ukulele"), names("guitar"), "acoustic instruments have no amp");
    assert_eq!(names("electric_guitar"), ["ring", "tone", "body", "gain", "drive"]);
    assert_eq!(names("bass_guitar"), names("electric_guitar"));
    let ring = |kind: &str| params(kind)[0].default;
    assert!(ring("ukulele") < ring("guitar") && ring("guitar") < ring("electric_guitar"), "a ukulele rings shortest, an electric longest");
    assert_eq!(params("electric_guitar")[4].default, 0.0, "the electric guitar is clean until Drive is turned up");
}

#[test]
fn zero_volume_is_silent() {
    let factory = &factories()[0];
    let sound = play(factory.as_ref(), &Params::from([("gain".to_string(), 0.0)]), 52);
    assert!(sound.iter().all(|v| *v == 0.0));
}
