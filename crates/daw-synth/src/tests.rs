//! The synth as an `Instrument`: catalog description, parameters, notes and controllers through the contract.

use daw_contracts::{Bpm, TimeSignature};

use super::*;

const SR: usize = 48_000;

fn make(params: &[(&str, f32)]) -> Box<dyn Instrument> {
    let map: Params = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    let mut synth = factory().create(&map, None).unwrap();
    synth.prepare(StreamConfig { sample_rate: SR as u32, channels: 2 });
    synth
}

/// `frames` stereo frames rendered into a silent buffer.
fn render(synth: &mut dyn Instrument, frames: usize) -> Vec<f32> {
    let mut buffer = vec![0.0; frames * 2];
    synth.render(&mut AudioBlock::new(&mut buffer, 2));
    buffer
}

fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
}

/// Frequency of the left channel from the number of upward zero crossings.
fn frequency(stereo: &[f32]) -> f32 {
    let left: Vec<f32> = stereo.chunks(2).map(|f| f[0]).collect();
    let rising = left.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    rising as f32 * SR as f32 / left.len() as f32
}

const OPEN_SINE: [(&str, f32); 4] = [("waveform", 0.0), ("high_cut", 18_000.0), ("gain", 100.0), ("sustain", 1.0)];

#[test]
fn describes_itself_with_the_envelope_names_and_a_waveform_choice() {
    let f = factory();
    assert_eq!(f.kind(), SYNTH_KIND);
    let params = f.describe();
    let names: Vec<_> = params.iter().map(|p| p.name).collect();
    assert_eq!(
        names,
        [
            "waveform",
            "osc2_level",
            "osc2_waveform",
            "osc2_semitones",
            "osc2_detune",
            "unison",
            "unison_detune",
            "unison_width",
            "attack",
            "hold",
            "decay",
            "sustain",
            "release",
            "low_cut",
            "high_cut",
            "resonance",
            "filter_env",
            "filter_attack",
            "filter_decay",
            "pitch_env",
            "pitch_decay",
            "mono",
            "glide",
            "lfo_shape",
            "lfo_rate",
            "lfo_sync",
            "lfo_pitch",
            "lfo_filter",
            "lfo_volume",
            "gain",
        ]
    );
    let waveform = &params[0];
    assert_eq!((waveform.kind, waveform.choices.len(), waveform.max), (ParamKind::Choice, 5, 4.0));
    for p in &params {
        assert!(p.min <= p.default && p.default <= p.max, "{}", p.name);
        assert!(!p.label.en.is_empty() && !p.label.ru.is_empty(), "{}", p.name);
        if p.kind == ParamKind::Choice {
            assert_eq!(p.max as usize + 1, p.choices.len(), "every option of {} has a label", p.name);
        }
    }
    assert!(f.rebuild_on().is_empty(), "every parameter arrives through set_param");
}

#[test]
fn silent_until_a_note_and_the_same_sound_in_both_channels() {
    let mut synth = make(&[]);
    assert_eq!(peak(&render(synth.as_mut(), 1024)), 0.0);
    synth.note_on(60, 1.0);
    let out = render(synth.as_mut(), 4_800);
    assert!(peak(&out) > 0.01);
    assert!(out.chunks(2).all(|f| f[0] == f[1] && f[0].is_finite()));
}

#[test]
fn render_adds_to_what_is_already_in_the_buffer() {
    let mut synth = make(&[]);
    synth.note_on(60, 1.0);
    let mut buffer = vec![0.5; 512];
    synth.render(&mut AudioBlock::new(&mut buffer, 2));
    assert!(buffer.iter().all(|v| (v - 0.5).abs() <= 1.0));
    assert!(buffer.iter().any(|v| (v - 0.5).abs() > 1e-4), "the synth sound is added to the 0.5 already there");
}

#[test]
fn note_off_fades_out_within_the_release_time() {
    let mut synth = make(&[("release", 0.1)]);
    synth.note_on(48, 1.0);
    render(synth.as_mut(), 9_600);
    synth.note_off(48);
    render(synth.as_mut(), SR / 8);
    assert!(peak(&render(synth.as_mut(), 4_800)) < 1e-3);
}

#[test]
fn all_notes_off_silences_a_chord() {
    let mut synth = make(&[]);
    for note in 40..60 {
        synth.note_on(note, 0.8);
    }
    render(synth.as_mut(), 2_048);
    synth.all_notes_off();
    render(synth.as_mut(), SR);
    assert!(peak(&render(synth.as_mut(), 512)) < 1e-3);
}

#[test]
fn waveform_choice_changes_the_sound() {
    let play = |waveform: f32| {
        let mut synth = make(&[("waveform", waveform), ("high_cut", 18_000.0)]);
        synth.note_on(57, 1.0);
        render(synth.as_mut(), 4_800)
    };
    assert_ne!(play(0.0), play(3.0));
}

#[test]
fn pitch_bend_and_mod_wheel_arrive_as_controllers() {
    let mut synth = make(&OPEN_SINE);
    synth.note_on(69, 1.0);
    render(synth.as_mut(), 4_800);
    synth.controller(ControllerKind::PitchBend, 1.0);
    let up = frequency(&render(synth.as_mut(), SR));
    assert!((up - engine::midi_to_hz(71)).abs() < 2.0, "{up}");

    let level = |modulation: f32| {
        let mut synth = make(&[("waveform", 0.0), ("high_cut", 100.0), ("sustain", 1.0), ("gain", 100.0)]);
        synth.controller(ControllerKind::Cc { number: CC_MODULATION }, modulation);
        synth.note_on(81, 1.0);
        peak(&render(synth.as_mut(), 9_600)[4_800..])
    };
    assert!(level(1.0) > level(0.0) * 4.0, "the mod wheel opens the filter");
}

#[test]
fn low_cut_removes_lows_and_high_cut_removes_highs() {
    let level = |note: u8, low_cut: f32, high_cut: f32| {
        let mut synth = make(&[("waveform", 0.0), ("low_cut", low_cut), ("high_cut", high_cut), ("sustain", 1.0), ("gain", 100.0)]);
        synth.note_on(note, 1.0);
        peak(&render(synth.as_mut(), 9_600)[4_800..])
    };
    assert!(level(45, 1_000.0, 18_000.0) < level(45, 20.0, 18_000.0) * 0.3, "the low cut removes a low note");
    assert!(level(93, 1_000.0, 18_000.0) > level(93, 20.0, 18_000.0) * 0.7, "and keeps a high one");
    assert!(level(93, 20.0, 200.0) < level(93, 20.0, 18_000.0) * 0.3, "the high cut removes a high note");
}

#[test]
fn volume_change_glides_instead_of_jumping() {
    let mut synth = make(&OPEN_SINE);
    synth.note_on(69, 1.0);
    render(synth.as_mut(), 4_800);
    synth.set_param("gain", 0.0);
    let out = render(synth.as_mut(), SR / 10);
    assert!(peak(&out[..200]) > 0.5, "right after the change the old volume is still heard");
    assert!(peak(&out[out.len() - 200..]) < 1e-6, "after the glide it is silent");
}

#[test]
fn a_new_synth_starts_at_its_volume_and_unknown_params_are_ignored() {
    let mut synth = make(&OPEN_SINE);
    synth.set_param("no_such_param", 5.0);
    synth.note_on(69, 1.0);
    let out = render(synth.as_mut(), 4_800);
    assert!(peak(&out[2_400..]) > 0.9, "full volume from the start, without gliding up from zero");
}

#[test]
fn out_of_range_values_are_clamped() {
    let mut synth = make(&[("high_cut", 1e9), ("gain", 1e9), ("waveform", 42.0), ("sustain", 7.0)]);
    synth.note_on(60, 1.0);
    let out = render(synth.as_mut(), 4_800);
    assert!(out.iter().all(|v| v.is_finite() && v.abs() < 1.35));
}

#[test]
fn unison_width_puts_the_copies_apart_in_the_two_channels() {
    let mut synth = make(&[("unison", 5.0), ("unison_detune", 30.0), ("unison_width", 100.0), ("high_cut", 18_000.0)]);
    synth.note_on(57, 1.0);
    let out = render(synth.as_mut(), 9_600);
    assert!(out.chunks(2).any(|f| (f[0] - f[1]).abs() > 0.01), "a stereo sound");
}

#[test]
fn a_mono_synth_slides_between_held_keys() {
    let mut synth = make(&[("mono", 1.0), ("glide", 0.1), ("waveform", 0.0), ("high_cut", 18_000.0), ("sustain", 1.0), ("gain", 100.0)]);
    synth.note_on(57, 1.0);
    render(synth.as_mut(), SR / 4);
    synth.note_on(69, 1.0);
    render(synth.as_mut(), SR / 2);
    assert!((frequency(&render(synth.as_mut(), SR / 2)) - 440.0).abs() < 3.0, "arrived at A4");
    synth.note_off(69);
    render(synth.as_mut(), SR / 2);
    assert!((frequency(&render(synth.as_mut(), SR / 2)) - 220.0).abs() < 3.0, "back at A3, still held");
}

#[test]
fn a_synced_lfo_takes_the_tempo_from_the_song() {
    let mut synth = make(&[
        ("waveform", 0.0),
        ("high_cut", 18_000.0),
        ("sustain", 1.0),
        ("gain", 100.0),
        ("lfo_shape", 3.0),
        ("lfo_sync", 3.0),
        ("lfo_volume", 100.0),
    ]);
    let tempo = Bpm::try_from(120.0).unwrap();
    let timing = BlockTiming {
        frames: SR,
        playing: true,
        start_tick: 0.0,
        ticks_per_frame: tempo.ticks_per_frame(SR as u32),
        tempo,
        time_signature: TimeSignature::default(),
        signature_start: 0.0,
        loop_region: None,
    };
    synth.set_timing(&timing);
    synth.note_on(69, 1.0);
    let out = render(synth.as_mut(), SR);
    let window = |from: f32, to: f32| peak(&out[(from * SR as f32) as usize * 2..(to * SR as f32) as usize * 2]);
    assert!(window(0.05, 0.2) > 0.9 && window(0.3, 0.45) < 1e-3 && window(0.55, 0.7) > 0.9);
}
