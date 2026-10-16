//! The sound core: envelopes over time, pitch, the filter and its envelope, unison, mono slides and the LFO.

use super::*;

const SR: u32 = 48_000;

fn engine() -> Engine {
    let mut e = Engine::default();
    e.prepare(SR as f32);
    e
}

/// `frames` frames, left channel.
fn render(e: &mut Engine, s: &Settings, frames: usize) -> Vec<f32> {
    (0..frames).map(|_| e.next_frame(s)[0]).collect()
}

fn render_stereo(e: &mut Engine, s: &Settings, frames: usize) -> Vec<[f32; 2]> {
    (0..frames).map(|_| e.next_frame(s)).collect()
}

fn seconds(s: f32) -> usize {
    (s * SR as f32) as usize
}

fn peak(buffer: &[f32]) -> f32 {
    buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
}

/// Peak in the window `[from, to)` seconds from the start of the rendered sound.
fn window_peak(out: &[f32], from: f32, to: f32) -> f32 {
    peak(&out[seconds(from)..seconds(to)])
}

/// Frequency from the number of upward zero crossings.
fn frequency(out: &[f32]) -> f32 {
    let rising = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    rising as f32 * SR as f32 / out.len() as f32
}

fn open_sine() -> Settings {
    Settings {
        waveform: Waveform::Sine,
        high_cut_hz: 18_000.0,
        gain: 1.0,
        envelope: Ahdsr { attack: 0.0005, sustain: 1.0, ..Ahdsr::default() },
        ..Settings::default()
    }
}

#[test]
fn silent_without_notes_and_audible_with_one() {
    let (mut e, s) = (engine(), Settings::default());
    assert_eq!(peak(&render(&mut e, &s, 1024)), 0.0);
    e.note_on(60, 1.0, &s);
    let out = render(&mut e, &s, 4800);
    assert!(peak(&out) > 0.01);
    assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
}

#[test]
fn note_off_decays_to_silence() {
    let (mut e, s) = (engine(), Settings::default());
    e.note_on(48, 1.0, &s);
    render(&mut e, &s, 9600);
    e.note_off(48, &s);
    render(&mut e, &s, SR as usize / 4);
    assert!(peak(&render(&mut e, &s, 4800)) < 1e-3);
}

#[test]
fn plays_chords_and_survives_voice_overflow() {
    let (mut e, s) = (engine(), Settings::default());
    for note in 40..60 {
        e.note_on(note, 0.8, &s);
    }
    assert!(render(&mut e, &s, 2048).iter().all(|v| v.is_finite()));
    e.all_notes_off(s.envelope.release);
    render(&mut e, &s, SR as usize);
    assert!(peak(&render(&mut e, &s, 512)) < 1e-3);
}

#[test]
fn hold_keeps_the_maximum_before_decay_starts() {
    let s = Settings { envelope: Ahdsr { attack: 0.001, hold: 0.2, decay: 0.05, sustain: 0.2, release: 0.1 }, ..open_sine() };
    let mut e = engine();
    e.note_on(69, 1.0, &s);
    let out = render(&mut e, &s, seconds(0.5));
    assert!(window_peak(&out, 0.05, 0.19) > 0.85, "during hold the level is at maximum");
    let after = window_peak(&out, 0.35, 0.5);
    assert!((after - 0.2).abs() < 0.04, "after decay sustain 0.2 remains, not {after}");
}

#[test]
fn release_lasts_the_set_time_regardless_of_the_level_it_starts_from() {
    for sustain in [1.0, 0.3] {
        let s = Settings { envelope: Ahdsr { attack: 0.001, decay: 0.001, sustain, release: 0.2, hold: 0.0 }, ..open_sine() };
        let mut e = engine();
        e.note_on(69, 1.0, &s);
        render(&mut e, &s, SR as usize / 2);
        e.note_off(69, &s);
        let out = render(&mut e, &s, SR as usize / 2);
        assert!(window_peak(&out, 0.0, 0.1) > 0.05, "at the start of release the sound is still audible (sustain {sustain})");
        assert!(window_peak(&out, 0.22, 0.5) < 1e-3, "after 0.2 s silence (sustain {sustain})");
    }
}

#[test]
fn pitch_bend_shifts_the_pitch_by_up_to_two_semitones() {
    let s = open_sine();
    let mut e = engine();
    e.note_on(69, 1.0, &s);
    render(&mut e, &s, 4800);
    let straight = frequency(&render(&mut e, &s, SR as usize));
    e.set_pitch_bend(1.0);
    let up = frequency(&render(&mut e, &s, SR as usize));
    e.set_pitch_bend(-1.0);
    let down = frequency(&render(&mut e, &s, SR as usize));
    assert!((straight - 440.0).abs() < 2.0, "{straight}");
    assert!((up - midi_to_hz(71)).abs() < 2.0, "{up}");
    assert!((down - midi_to_hz(67)).abs() < 2.0, "{down}");
}

/// Peak of note `note` after it settles, with settings `s`.
fn settled_level(s: &Settings, note: u8) -> f32 {
    let mut e = engine();
    e.note_on(note, 1.0, s);
    peak(&render(&mut e, s, 9600)[4800..])
}

#[test]
fn modulation_wheel_opens_the_filter() {
    // A sine at 880 Hz with a 100 Hz cutoff is almost inaudible; full modulation raises the cutoff to 1600 Hz.
    let s = Settings { high_cut_hz: 100.0, ..open_sine() };
    let closed = settled_level(&s, 81);
    let mut e = engine();
    e.set_modulation(1.0);
    e.note_on(81, 1.0, &s);
    let open = peak(&render(&mut e, &s, 9600)[4800..]);
    assert!(open > closed * 4.0, "without modulation {closed}, with modulation {open}");
}

#[test]
fn resonance_lifts_a_note_at_the_cutoff() {
    let s = Settings { high_cut_hz: 880.0, ..open_sine() };
    let smooth = settled_level(&s, 81);
    let sharp = settled_level(&Settings { resonance: 1.0, ..s }, 81);
    assert!(sharp > smooth * 4.0, "{smooth} → {sharp}");
}

#[test]
fn the_filter_envelope_opens_the_filter_at_the_start_of_the_note() {
    let s = Settings { high_cut_hz: 100.0, filter_env_octaves: 6.0, filter_attack: 0.001, filter_decay: 0.2, ..open_sine() };
    let mut e = engine();
    e.note_on(93, 1.0, &s);
    let out = render(&mut e, &s, seconds(0.5));
    let (start, end) = (window_peak(&out, 0.01, 0.05), window_peak(&out, 0.4, 0.5));
    assert!(start > end * 5.0, "a pluck: open {start}, closed {end}");
}

#[test]
fn the_pitch_envelope_starts_above_the_note_and_settles_on_it() {
    let s = Settings { pitch_env_semitones: 12.0, pitch_decay: 0.3, ..open_sine() };
    let mut e = engine();
    e.note_on(69, 1.0, &s);
    let early = frequency(&render(&mut e, &s, seconds(0.02)));
    render(&mut e, &s, seconds(0.6));
    let late = frequency(&render(&mut e, &s, seconds(0.5)));
    assert!(early > 700.0, "the start is near an octave up: {early}");
    assert!((late - 440.0).abs() < 3.0, "{late}");
}

#[test]
fn a_second_oscillator_an_octave_up_doubles_the_crossings() {
    let s = Settings { osc2: Waveform::Sine, osc2_level: 1.0, osc2_ratio: 2.0, ..open_sine() };
    let mut e = engine();
    e.note_on(69, 1.0, &s);
    render(&mut e, &s, 4800);
    let out = render(&mut e, &s, SR as usize);
    assert!((frequency(&out) - 880.0).abs() < 4.0, "sin x + sin 2x crosses zero upward twice a cycle: {}", frequency(&out));
    assert!(peak(&out) <= 1.0, "the two oscillators share the level");
}

#[test]
fn noise_has_no_pitch() {
    let s = Settings { waveform: Waveform::Noise, ..open_sine() };
    let mut e = engine();
    e.note_on(40, 1.0, &s);
    render(&mut e, &s, 4800);
    let out = render(&mut e, &s, SR as usize);
    assert!(frequency(&out) > 2_000.0, "white noise crosses zero far more often than a low note: {}", frequency(&out));
    // A filter open near the top lets a sharp signal overshoot a little (at most 1.31 at 18 kHz).
    assert!(out.iter().all(|v| v.abs() < 1.35));
}

#[test]
fn unison_spreads_detuned_copies_across_the_stereo_field() {
    let mono = settings_stereo(&open_sine());
    assert!(mono.iter().all(|[l, r]| l == r), "one copy sits in the centre");
    let wide = settings_stereo(&Settings { unison: Unison::new(5.0, 30.0, 1.0), ..open_sine() });
    assert!(wide.iter().any(|[l, r]| (l - r).abs() > 0.05), "copies spread to the sides");
    assert!(wide.iter().all(|[l, r]| l.abs() < 1.6 && r.abs() < 1.6), "the loudness stays about the same");
    let narrow = settings_stereo(&Settings { unison: Unison::new(5.0, 30.0, 0.0), ..open_sine() });
    assert!(narrow.iter().all(|[l, r]| l == r), "zero width keeps them in the centre");
}

fn settings_stereo(s: &Settings) -> Vec<[f32; 2]> {
    let mut e = engine();
    e.note_on(57, 1.0, s);
    render_stereo(&mut e, s, 9600)
}

#[test]
fn unison_copies_are_detuned_symmetrically() {
    let unison = Unison::new(3.0, 20.0, 0.5);
    assert_eq!(unison.count, 3);
    assert!((unison.ratios[0] * unison.ratios[2] - 1.0).abs() < 1e-5 && unison.ratios[1] == 1.0);
    assert!((unison.ratios[2].log2() * 1200.0 - 10.0).abs() < 1e-3, "the outer copies are ±10 cents");
    assert_eq!(Unison::new(42.0, 0.0, 0.0).count, MAX_UNISON);
    assert_eq!(Unison::new(0.0, 0.0, 0.0).count, 1);
}

fn mono(glide: f32) -> Settings {
    Settings { mono: true, glide, ..open_sine() }
}

#[test]
fn mono_legato_slides_to_the_new_note_without_restarting_the_envelope() {
    let s = Settings { envelope: Ahdsr { attack: 0.3, sustain: 1.0, ..Ahdsr::default() }, ..mono(0.2) };
    let mut e = engine();
    e.note_on(57, 1.0, &s);
    let before = window_peak(&render(&mut e, &s, seconds(0.5)), 0.4, 0.5);
    e.note_on(69, 1.0, &s);
    let slide = render(&mut e, &s, seconds(0.6));
    let middle = frequency(&slide[seconds(0.02)..seconds(0.1)]);
    assert!(middle > 230.0 && middle < 430.0, "halfway between A3 and A4: {middle}");
    assert!((frequency(&slide[seconds(0.4)..]) - 440.0).abs() < 3.0);
    assert!(window_peak(&slide, 0.0, 0.05) > before * 0.9, "no new attack from silence");
}

#[test]
fn mono_release_slides_back_to_the_note_still_held() {
    let s = mono(0.0);
    let mut e = engine();
    e.note_on(57, 1.0, &s);
    e.note_on(69, 1.0, &s);
    render(&mut e, &s, 4800);
    assert!((frequency(&render(&mut e, &s, SR as usize / 2)) - 440.0).abs() < 3.0);
    e.note_off(69, &s);
    assert!((frequency(&render(&mut e, &s, SR as usize / 2)) - 220.0).abs() < 3.0, "back to A3");
    e.note_off(57, &s);
    render(&mut e, &s, SR as usize / 2);
    assert!(peak(&render(&mut e, &s, 512)) < 1e-3, "the last key ends the note");
}

#[test]
fn a_new_mono_note_after_the_release_starts_fresh_without_sliding() {
    let s = mono(0.5);
    let mut e = engine();
    e.note_on(57, 1.0, &s);
    render(&mut e, &s, 4800);
    e.note_off(57, &s);
    e.note_on(69, 1.0, &s);
    render(&mut e, &s, 480);
    assert!((frequency(&render(&mut e, &s, SR as usize / 4)) - 440.0).abs() < 4.0, "no slide from a released note");
}

#[test]
fn poly_mode_ignores_glide() {
    let s = Settings { glide: 1.0, ..open_sine() };
    let mut e = engine();
    e.note_on(57, 1.0, &s);
    e.note_on(69, 1.0, &s);
    e.note_off(57, &s);
    render(&mut e, &s, seconds(0.3));
    assert!((frequency(&render(&mut e, &s, SR as usize / 4)) - 440.0).abs() < 4.0);
}

#[test]
fn the_lfo_bends_the_pitch_for_vibrato() {
    let s = Settings { lfo: LfoSettings { rate_hz: 1.0, pitch_cents: 100.0, ..LfoSettings::default() }, ..open_sine() };
    let mut e = engine();
    e.note_on(69, 1.0, &s);
    let out = render(&mut e, &s, SR as usize);
    // A sine LFO at 1 Hz: up around a quarter of its cycle, down around three quarters.
    let (up, down) = (frequency(&out[seconds(0.15)..seconds(0.35)]), frequency(&out[seconds(0.65)..seconds(0.85)]));
    assert!(up > 455.0 && down < 425.0, "{up} / {down}");
}

#[test]
fn a_synced_lfo_follows_the_song_tempo() {
    let s = Settings {
        lfo: LfoSettings { shape: LfoShape::Square, sync_beats: Some(1.0), volume: 1.0, ..LfoSettings::default() },
        ..open_sine()
    };
    let mut e = engine();
    // 120 BPM: one beat is half a second, so the square is on for 0.25 s and off for 0.25 s.
    e.set_timing(0.0, 120.0 / 60.0 / f64::from(SR), true, s.lfo.sync_beats);
    e.note_on(69, 1.0, &s);
    let out = render(&mut e, &s, SR as usize);
    assert!(window_peak(&out, 0.05, 0.2) > 0.9);
    assert!(window_peak(&out, 0.3, 0.45) < 1e-3);
    assert!(window_peak(&out, 0.55, 0.7) > 0.9, "the next beat");

    // A segment starting half a beat into the song puts the LFO in the off half.
    e.set_timing(10.5, 120.0 / 60.0 / f64::from(SR), true, s.lfo.sync_beats);
    assert!(peak(&render(&mut e, &s, seconds(0.2))[seconds(0.02)..]) < 1e-3);
}

#[test]
fn reset_returns_the_wheels_to_rest() {
    let mut e = engine();
    e.set_pitch_bend(1.0);
    e.set_modulation(1.0);
    e.reset();
    assert_eq!((e.bend_ratio, e.modulation), (1.0, 0.0));
}

#[test]
fn midi_pitch_is_correct() {
    assert!((midi_to_hz(69) - 440.0).abs() < 1e-3);
    assert!((midi_to_hz(57) - 220.0).abs() < 1e-2);
}
