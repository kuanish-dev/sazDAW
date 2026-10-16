//! The synth's sound core: 8 voices, oscillator, AHDSR envelope, highpass (low cut) and lowpass (high cut); pitch wheel (pitch bend, ±2 semitones)
//! and mod wheel (CC1: opens the filter up to four octaves above the cutoff).
//! Pure math without the contracts: the `Instrument` glue in `lib.rs` feeds it notes and settings.

use std::f32::consts::TAU;

pub(crate) const VOICES: usize = 8;
/// How far pitch bend at the end of the wheel shifts the pitch, in semitones.
pub(crate) const BEND_RANGE_SEMITONES: f32 = 2.0;
/// The low cut at or below this frequency is off (its minimum; a little above it for the glide's rounding).
const LOW_CUT_OFF_HZ: f32 = 20.01;

/// How many octaves the mod wheel at its maximum raises the filter cutoff.
pub(crate) const MODULATION_OCTAVES: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

impl Waveform {
    /// In the order of the `waveform` parameter options.
    pub(crate) const ALL: [Waveform; 4] = [Waveform::Sine, Waveform::Triangle, Waveform::Saw, Waveform::Square];

    /// The option by its index (the parameter value); out-of-range values take the nearest option.
    pub(crate) fn from_index(value: f32) -> Self {
        Self::ALL[(value.round().max(0.0) as usize).min(Self::ALL.len() - 1)]
    }
}

/// Current parameter values (seconds, Hz, linear gain; sustain is a level `0..=1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Settings {
    pub waveform: Waveform,
    pub attack: f32,
    /// How many seconds the level holds at maximum after the attack.
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Lowpass corner (the mod wheel raises it).
    pub high_cut_hz: f32,
    /// Highpass corner: removes lows below it; 20 Hz leaves the sound untouched.
    pub low_cut_hz: f32,
    pub gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            attack: 0.005,
            hold: 0.0,
            decay: 0.15,
            sustain: 0.6,
            release: 0.12,
            high_cut_hz: 1_800.0,
            low_cut_hz: 20.0,
            gain: 0.25,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    note: u8,
    phase: f32,
    step: f32,
    level: f32,
    velocity: f32,
    stage: Stage,
    age: u64,
    /// How many frames to keep holding the maximum (the Hold stage).
    hold_left: f32,
    /// Level drop per frame in the Release stage: from the current level to zero in exactly `release` seconds.
    release_step: f32,
}

impl Voice {
    const IDLE: Voice =
        Voice { note: 0, phase: 0.0, step: 0.0, level: 0.0, velocity: 0.0, stage: Stage::Idle, age: 0, hold_left: 0.0, release_step: 0.0 };
}

pub(crate) fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(note) - 69.0) / 12.0)
}

/// Waveform by phase `0..TAU`; the result is in `[-1, 1]`.
pub(crate) fn oscillator(waveform: Waveform, phase: f32) -> f32 {
    let t = phase / TAU;
    match waveform {
        Waveform::Sine => phase.sin(),
        Waveform::Triangle => 4.0 * (t - 0.5).abs() - 1.0,
        Waveform::Saw => 2.0 * t - 1.0,
        Waveform::Square => {
            if t < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
    }
}

pub(crate) struct Engine {
    sample_rate: f32,
    voices: [Voice; VOICES],
    filter_state: f32,
    /// Lowpass of the mix at the low cut; subtracting it leaves the highpassed signal.
    low_state: f32,
    clock: u64,
    /// How much pitch bend speeds up the phase of all voices (1 means the wheel is centered).
    bend_ratio: f32,
    /// Mod wheel, `0..=1`.
    modulation: f32,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            voices: [Voice::IDLE; VOICES],
            filter_state: 0.0,
            low_state: 0.0,
            clock: 0,
            bend_ratio: 1.0,
            modulation: 0.0,
        }
    }
}

impl Engine {
    pub(crate) fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.reset();
    }

    pub(crate) fn reset(&mut self) {
        self.voices = [Voice::IDLE; VOICES];
        self.filter_state = 0.0;
        self.low_state = 0.0;
        self.bend_ratio = 1.0;
        self.modulation = 0.0;
    }

    /// Pitch wheel: `-1..=1` (0 is the middle), the end is ±[`BEND_RANGE_SEMITONES`].
    pub(crate) fn set_pitch_bend(&mut self, bend: f32) {
        self.bend_ratio = 2f32.powf(bend.clamp(-1.0, 1.0) * BEND_RANGE_SEMITONES / 12.0);
    }

    /// Mod wheel: `0..=1`.
    pub(crate) fn set_modulation(&mut self, amount: f32) {
        self.modulation = amount.clamp(0.0, 1.0);
    }

    /// Linear envelope increment per frame for a given duration.
    fn rate(&self, seconds: f32) -> f32 {
        1.0 / (seconds.max(0.0005) * self.sample_rate)
    }

    pub(crate) fn note_on(&mut self, note: u8, velocity: f32) {
        self.clock += 1;
        // A free voice, otherwise the oldest one.
        let index = self
            .voices
            .iter()
            .position(|v| v.stage == Stage::Idle)
            .or_else(|| self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map(|(i, _)| i))
            .unwrap_or(0);
        self.voices[index] = Voice {
            note,
            phase: 0.0,
            step: TAU * midi_to_hz(note) / self.sample_rate,
            level: 0.0,
            velocity: velocity.clamp(0.0, 1.0),
            stage: Stage::Attack,
            age: self.clock,
            hold_left: 0.0,
            release_step: 0.0,
        };
    }

    pub(crate) fn note_off(&mut self, note: u8, release: f32) {
        let rate = self.rate(release);
        for voice in &mut self.voices {
            if voice.note == note && !matches!(voice.stage, Stage::Idle | Stage::Release) {
                voice.stage = Stage::Release;
                voice.release_step = voice.level * rate;
            }
        }
    }

    pub(crate) fn all_notes_off(&mut self, release: f32) {
        let rate = self.rate(release);
        for voice in &mut self.voices {
            if voice.stage != Stage::Idle {
                voice.stage = Stage::Release;
                voice.release_step = voice.level * rate;
            }
        }
    }

    fn advance_envelope(&self, voice: &mut Voice, s: &Settings) {
        match voice.stage {
            Stage::Idle | Stage::Sustain => {}
            Stage::Attack => {
                voice.level += self.rate(s.attack);
                if voice.level >= 1.0 {
                    voice.level = 1.0;
                    if s.hold > 0.0 {
                        voice.hold_left = s.hold * self.sample_rate;
                        voice.stage = Stage::Hold;
                    } else {
                        voice.stage = Stage::Decay;
                    }
                }
            }
            Stage::Hold => {
                voice.hold_left -= 1.0;
                if voice.hold_left <= 0.0 {
                    voice.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                voice.level -= self.rate(s.decay) * (1.0 - s.sustain).max(0.0001);
                if voice.level <= s.sustain {
                    voice.level = s.sustain;
                    voice.stage = Stage::Sustain;
                }
            }
            Stage::Release => {
                voice.level -= voice.release_step;
                if voice.level <= 0.0 {
                    *voice = Voice::IDLE;
                }
            }
        }
    }

    /// One frame (mono; duplicating to channels is the caller's job).
    pub(crate) fn next_sample(&mut self, s: &Settings) -> f32 {
        // Modulation raises the cutoff, but not above almost half the sample rate: the filter cuts nothing there anyway.
        let cutoff = (s.high_cut_hz * 2f32.powf(self.modulation * MODULATION_OCTAVES)).min(0.45 * self.sample_rate);
        let alpha = 1.0 - (-TAU * cutoff / self.sample_rate).exp();
        // At its minimum the low cut is off, not a 20 Hz filter: even that slightly reshapes a square (overshoot).
        let low_alpha = if s.low_cut_hz <= LOW_CUT_OFF_HZ {
            0.0
        } else {
            1.0 - (-TAU * s.low_cut_hz.min(0.45 * self.sample_rate) / self.sample_rate).exp()
        };
        let mut mix = 0.0;
        let mut voices = self.voices;
        for voice in &mut voices {
            if voice.stage == Stage::Idle {
                continue;
            }
            self.advance_envelope(voice, s);
            if voice.stage == Stage::Idle {
                continue;
            }
            mix += oscillator(s.waveform, voice.phase) * voice.level * voice.velocity;
            voice.phase = (voice.phase + voice.step * self.bend_ratio) % TAU;
        }
        self.voices = voices;
        self.low_state += low_alpha * (mix - self.low_state);
        self.filter_state += alpha * ((mix - self.low_state) - self.filter_state);
        self.filter_state * s.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn engine() -> Engine {
        let mut e = Engine::default();
        e.prepare(SR as f32);
        e
    }

    fn render(e: &mut Engine, s: &Settings, frames: usize) -> Vec<f32> {
        (0..frames).map(|_| e.next_sample(s)).collect()
    }

    fn peak(buffer: &[f32]) -> f32 {
        buffer.iter().fold(0.0, |a, b| a.max(b.abs()))
    }

    /// Peak in the window `[from, to)` seconds from the start of the rendered sound.
    fn window_peak(out: &[f32], from: f32, to: f32) -> f32 {
        peak(&out[(from * SR as f32) as usize..(to * SR as f32) as usize])
    }

    fn open_sine() -> Settings {
        Settings { waveform: Waveform::Sine, high_cut_hz: 18_000.0, gain: 1.0, ..Settings::default() }
    }

    #[test]
    fn silent_without_notes_and_audible_with_one() {
        let (mut e, s) = (engine(), Settings::default());
        assert_eq!(peak(&render(&mut e, &s, 1024)), 0.0);
        e.note_on(60, 1.0);
        let out = render(&mut e, &s, 4800);
        assert!(peak(&out) > 0.01);
        assert!(out.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn note_off_decays_to_silence() {
        let (mut e, s) = (engine(), Settings::default());
        e.note_on(48, 1.0);
        render(&mut e, &s, 9600);
        e.note_off(48, s.release);
        render(&mut e, &s, SR as usize / 4);
        assert!(peak(&render(&mut e, &s, 4800)) < 1e-3);
    }

    #[test]
    fn plays_chords_and_survives_voice_overflow() {
        let (mut e, s) = (engine(), Settings::default());
        for note in 40..60 {
            e.note_on(note, 0.8);
        }
        assert!(render(&mut e, &s, 2048).iter().all(|v| v.is_finite()));
        e.all_notes_off(s.release);
        render(&mut e, &s, SR as usize);
        assert!(peak(&render(&mut e, &s, 512)) < 1e-3);
    }

    #[test]
    fn hold_keeps_the_maximum_before_decay_starts() {
        let s = Settings { attack: 0.001, hold: 0.2, decay: 0.05, sustain: 0.2, ..open_sine() };
        let mut e = engine();
        e.note_on(69, 1.0);
        let out = render(&mut e, &s, (0.5 * SR as f32) as usize);
        assert!(window_peak(&out, 0.05, 0.19) > 0.85, "during hold the level is at maximum");
        let after = window_peak(&out, 0.35, 0.5);
        assert!((after - 0.2).abs() < 0.04, "after decay sustain 0.2 remains, not {after}");
    }

    #[test]
    fn release_lasts_the_set_time_regardless_of_the_level_it_starts_from() {
        for sustain in [1.0, 0.3] {
            let s = Settings { attack: 0.001, decay: 0.001, sustain, release: 0.2, ..open_sine() };
            let mut e = engine();
            e.note_on(69, 1.0);
            render(&mut e, &s, SR as usize / 2);
            e.note_off(69, s.release);
            let out = render(&mut e, &s, SR as usize / 2);
            assert!(window_peak(&out, 0.0, 0.1) > 0.05, "at the start of release the sound is still audible (sustain {sustain})");
            assert!(window_peak(&out, 0.22, 0.5) < 1e-3, "after 0.2 s silence (sustain {sustain})");
        }
    }

    /// Frequency from the number of upward zero crossings.
    fn frequency(out: &[f32]) -> f32 {
        let rising = out.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        rising as f32 * SR as f32 / out.len() as f32
    }

    #[test]
    fn pitch_bend_shifts_the_pitch_by_up_to_two_semitones() {
        let s = Settings { attack: 0.0005, sustain: 1.0, ..open_sine() };
        let mut e = engine();
        e.note_on(69, 1.0);
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

    #[test]
    fn modulation_wheel_opens_the_filter() {
        // A sine at 880 Hz with a 100 Hz cutoff is almost inaudible; full modulation raises the cutoff to 1600 Hz.
        let s = Settings { high_cut_hz: 100.0, sustain: 1.0, ..open_sine() };
        let level = |modulation: f32| {
            let mut e = engine();
            e.set_modulation(modulation);
            e.note_on(81, 1.0);
            peak(&render(&mut e, &s, 9600)[4800..])
        };
        let (closed, open) = (level(0.0), level(1.0));
        assert!(open > closed * 4.0, "without modulation {closed}, with modulation {open}");
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

    #[test]
    fn every_waveform_stays_within_one() {
        for waveform in Waveform::ALL {
            assert!((0..64).all(|i| oscillator(waveform, TAU * i as f32 / 64.0).abs() <= 1.0), "{waveform:?}");
        }
    }

    #[test]
    fn waveform_index_picks_the_nearest_option() {
        assert_eq!(Waveform::from_index(0.0), Waveform::Sine);
        assert_eq!(Waveform::from_index(2.4), Waveform::Saw);
        assert_eq!(Waveform::from_index(9.0), Waveform::Square);
        assert_eq!(Waveform::from_index(-3.0), Waveform::Sine);
    }
}
