//! The dev-only fixture plugin `test-plugins/synth` in the host: found as an instrument, sounds, fades, the window opens at
//! the right size. No `Synth.vst3` built — the test is skipped (build it: `scripts/build-test-plugins.sh`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::sync::{Mutex, MutexGuard};

use crate::support::{SYNTH, world};
use daw_contracts::sync::lock;
use daw_contracts::{AppLog, AudioBlock, ControllerKind, EditorTarget, Instrument, L10n, ParamKind, Params, PluginEditors, StreamConfig};

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    lock(&ONE_AT_A_TIME)
}

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

/// Renders `blocks` blocks and returns the peak (and checks that the sound has no NaN/inf).
fn render(instrument: &mut dyn Instrument, blocks: usize) -> f32 {
    let mut peak = 0.0f32;
    let mut buffer = vec![0.0f32; BLOCK * 2];
    for _ in 0..blocks {
        buffer.fill(0.0);
        instrument.render(&mut AudioBlock::new(&mut buffer, 2));
        assert!(buffer.iter().all(|v| v.is_finite()), "the sound has NaN/inf");
        peak = buffer.iter().fold(peak, |a, v| a.max(v.abs()));
    }
    peak
}

#[test]
fn synth_is_found_as_an_instrument_with_its_eight_parameters() {
    let _one = serial();
    let Some(world) = world("synth-scan", &[SYNTH]) else { return };
    assert_eq!(world.instruments.len(), 1);
    assert!(world.effects.is_empty());
    let (kind, params) = &world.instruments[0];
    assert_eq!(*kind, "vst3:synth");
    assert_eq!(params.len(), 8, "waveform, attack, hold, decay, sustain, release, cutoff, gain");
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn synth_parameters_are_in_plain_units_with_named_waveforms() {
    let _one = serial();
    let Some(world) = world("synth-units", &[SYNTH]) else { return };
    let params = world.instrument_factories[0].describe();
    let by_label =
        |label: &str| params.iter().find(|p| p.label.get() == label).unwrap_or_else(|| panic!("no \"{label}\" among {params:?}"));
    let waveform = by_label("Waveform");
    assert_eq!((waveform.kind, waveform.min, waveform.max, waveform.default), (ParamKind::Choice, 0.0, 3.0, 2.0));
    assert_eq!(waveform.choices.iter().map(L10n::get).collect::<Vec<_>>(), ["Sine", "Triangle", "Saw", "Square"]);
    let cutoff = by_label("Cutoff, Hz");
    assert!((cutoff.min - 40.0).abs() < 0.01 && (cutoff.max - 18_000.0).abs() < 0.5, "{cutoff:?}");
    let attack = by_label("Attack, s");
    assert!((attack.min - 0.0005).abs() < 1e-6 && (attack.max - 4.0).abs() < 1e-3, "{attack:?}");
    let sustain = by_label("Sustain, %");
    assert_eq!((sustain.min, sustain.max), (0.0, 100.0), "a fraction shown in percent is exposed in percent");
    by_label("Gain, %");
    for label in ["Hold, s", "Decay, s", "Release, s"] {
        by_label(label);
    }
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

/// Peak of a sounding saw note 60 at cutoff `cutoff` Hz.
fn saw_peak_with_cutoff(world: &crate::support::World, cutoff: f32) -> f32 {
    let params = world.instrument_factories[0].describe();
    let name = |label: &str| params.iter().find(|p| p.label.get() == label).unwrap().name.to_string();
    let settings = Params::from([(name("Waveform"), 2.0), (name("Cutoff, Hz"), cutoff)]);
    let mut synth = world.instrument_factories[0].create(&settings, None).expect("is created");
    synth.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    synth.note_on(60, 1.0);
    // The cutoff is smoothed from the default: measure after smoothing and the attack are over.
    render(&mut *synth, 60);
    render(&mut *synth, 20)
}

#[test]
fn synth_takes_values_in_plain_units() {
    let _one = serial();
    let Some(world) = world("synth-plain", &[SYNTH]) else { return };
    // 40 Hz cuts note 60 (262 Hz) almost entirely, 18000 Hz lets it through. If the values were taken as
    // normalized, both numbers would be clamped to 1.0 and the peaks would match.
    let (dark, bright) = (saw_peak_with_cutoff(&world, 40.0), saw_peak_with_cutoff(&world, 18_000.0));
    eprintln!("Synth: peak at cutoff 40 Hz {dark}, at 18000 Hz {bright}");
    assert!(bright > 0.01 && dark < bright * 0.5, "cutoff 40 Hz: {dark}, 18000 Hz: {bright}");
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn synth_plays_a_note_and_goes_silent_after_release() {
    let _one = serial();
    let Some(world) = world("synth-sound", &[SYNTH]) else { return };
    let mut synth = world.instrument_factories[0].create(&Params::new(), None).expect("is created");
    synth.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    assert_eq!(render(&mut *synth, 4), 0.0, "silence without notes");
    synth.note_on(60, 1.0);
    let peak = render(&mut *synth, 100);
    eprintln!("Synth: peak with a held note {peak}");
    assert!(peak > 0.01, "silence instead of a note: {peak}");
    synth.note_off(60);
    render(&mut *synth, 200);
    let tail = render(&mut *synth, 10);
    assert!(tail < 1e-3, "the sound did not fade after note_off: {tail}");
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

#[test]
fn synth_window_has_the_designed_size() {
    let _one = serial();
    let Some(world) = world("synth-window", &[SYNTH]) else { return };
    world.add_instrument_track(1, world.instruments[0].0);
    let session = world.editors.open(EditorTarget::Instrument { track: 1 }).expect("the window is created");
    assert_eq!((session.width, session.height), (720, 440));
    world.editors.close(session.id);
    assert!(world.log.recent(10).is_empty(), "log: {:?}", world.log.recent(10));
}

/// The left channel of `blocks` blocks in a row.
fn capture(instrument: &mut dyn Instrument, blocks: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(blocks * BLOCK);
    let mut buffer = vec![0.0f32; BLOCK * 2];
    for _ in 0..blocks {
        buffer.fill(0.0);
        instrument.render(&mut AudioBlock::new(&mut buffer, 2));
        out.extend(buffer.chunks_exact(2).map(|frame| frame[0]));
    }
    out
}

/// Frequency from the number of upward zero crossings.
fn frequency(out: &[f32]) -> f32 {
    let rising = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    rising as f32 * RATE as f32 / out.len() as f32
}

#[test]
fn pitch_bend_reaches_the_synth_through_its_midi_mapping() {
    let _one = serial();
    let Some(world) = world("synth-bend", &[SYNTH]) else { return };
    let params = world.instrument_factories[0].describe();
    let name = |label: &str| params.iter().find(|p| p.label.get() == label).unwrap().name.to_string();
    let settings = Params::from([(name("Waveform"), 0.0), (name("Cutoff, Hz"), 18_000.0), (name("Sustain, %"), 100.0)]);
    let mut synth = world.instrument_factories[0].create(&settings, None).expect("is created");
    synth.prepare(StreamConfig { sample_rate: RATE, channels: 2 });
    synth.note_on(69, 1.0);
    capture(&mut *synth, 40);
    let straight = frequency(&capture(&mut *synth, 190));
    synth.controller(ControllerKind::PitchBend, 1.0);
    capture(&mut *synth, 4);
    let bent = frequency(&capture(&mut *synth, 190));
    eprintln!("Synth: no bend {straight} Hz, wheel fully up {bent} Hz");
    assert!((straight - 440.0).abs() < 3.0, "{straight}");
    let whole_tone_up = 440.0 * 2f32.powf(2.0 / 12.0);
    assert!((bent - whole_tone_up).abs() < 3.0, "expected {whole_tone_up}, hear {bent}");
    synth.controller(ControllerKind::Cc { number: 1 }, 1.0);
    capture(&mut *synth, 2);
    drop(synth);
    assert!(world.log.recent(10).is_empty(), "controllers are mapped, no failures: {:?}", world.log.recent(10));
}
