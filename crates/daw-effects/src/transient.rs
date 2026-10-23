//! Transient shaper: changes how hard a sound starts (`attack`) and how long it rings (`sustain`) whatever its level, with
//! no threshold. Two pairs of envelope followers (`dynamics.rs: Follower`) on the louder channel measure both:
//! * the transient — the peak (instant attack, `PEAK_RELEASE_MS`) over the same peak followed slowly upward
//!   (`TRANSIENT_MS`) and at once downward: they differ only while the level jumps up, by how far it jumped;
//! * the tail — the level (a mean square over `LEVEL_MS`) held from its peak (`HOLD_RELEASE_MS`) over the level itself:
//!   they differ only while the level falls, by how far it fell.
//!
//! Both are in dB, ≥ 0, and on a steady sound both stay near 0 dB. The gain is `attack · transient + sustain · tail` dB
//! (each ±100 % of the measured amount), capped at ±`MAX_SHAPE_DB` and smoothed over `GAIN_MS`, then the output trim.
//! No lookahead: the first fraction of a millisecond of a hit is shaped as the peak follower catches it, so the
//! latency is 0. The graph in the interface runs the same followers on a model hit (`app/src/effects/transientShape.ts`):
//! a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::dynamics::Follower;
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param, db_to_gain, number, time_coefficient};

/// How long the peak follower holds a peak: long enough that a steady bass note does not read as transients.
const PEAK_RELEASE_MS: f32 = 250.0;
/// How long a transient lasts: the slow follower reaches a new level in about this time.
const TRANSIENT_MS: f32 = 20.0;
/// The level's mean-square window: smooth enough for the low end, quick enough to follow a tail.
const LEVEL_MS: f32 = 10.0;
/// How long the hold follower keeps the level's peak while the sound dies away.
const HOLD_RELEASE_MS: f32 = 300.0;
/// The most the shaper turns, dB either way.
const MAX_SHAPE_DB: f32 = 18.0;
/// Smoothing of the gain, so a fast change of it does not crackle.
const GAIN_MS: f32 = 1.0;
/// Below this level everything counts as silence, so the noise floor does not read as transients.
const FLOOR: f32 = 1e-5;

const PARAMS: [ParamInfo; 3] = [
    number("attack", L10n { en: "Attack, %", ru: "Атака, %" }, -100.0, 100.0, 0.0),
    number("sustain", L10n { en: "Sustain, %", ru: "Сустейн, %" }, -100.0, 100.0, 0.0),
    number("output", L10n { en: "Output, dB", ru: "Выход, дБ" }, -24.0, 12.0, 0.0),
];

pub struct Transient {
    sample_rate: f32,
    smoothing: f32,
    attack: Smoothed,
    sustain: Smoothed,
    output: Smoothed,
    peak: Follower,
    slow_peak: Follower,
    level_coefficient: f32,
    mean_square: f32,
    hold: Follower,
    gain: Follower,
}

impl Transient {
    fn update(&mut self) {
        let sr = self.sample_rate;
        self.smoothing = smooth_coefficient(sr);
        self.peak.set_times(0.0, PEAK_RELEASE_MS, sr);
        self.slow_peak.set_times(TRANSIENT_MS, 0.0, sr);
        self.level_coefficient = time_coefficient(LEVEL_MS, sr);
        self.hold.set_times(0.0, HOLD_RELEASE_MS, sr);
        self.gain.set_times(GAIN_MS, GAIN_MS, sr);
    }

    /// The gain in dB for one frame whose louder channel is `peak`.
    fn shape_db(&mut self, peak: f32, attack: f32, sustain: f32) -> f32 {
        let fast = self.peak.follow(peak).max(FLOOR);
        let slow = self.slow_peak.follow(fast).max(FLOOR);
        self.mean_square = peak * peak + self.level_coefficient * (self.mean_square - peak * peak);
        let level = self.mean_square.sqrt().max(FLOOR);
        let held = self.hold.follow(level).max(FLOOR);
        let transient = 20.0 * (fast / slow).log10();
        let tail = 20.0 * (held / level).log10();
        self.gain.follow((attack * transient + sustain * tail).clamp(-MAX_SHAPE_DB, MAX_SHAPE_DB))
    }
}

impl EffectKind for Transient {
    const KIND: &'static str = "transient";
    const LABEL: L10n = L10n { en: "Transient shaper", ru: "Транзиент-шейпер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut shaper = Self {
            sample_rate: config.sample_rate as f32,
            smoothing: 0.0,
            attack: Smoothed::new(0.0),
            sustain: Smoothed::new(0.0),
            output: Smoothed::new(1.0),
            peak: Follower::default(),
            slow_peak: Follower::default(),
            level_coefficient: 0.0,
            mean_square: 0.0,
            hold: Follower::default(),
            gain: Follower::default(),
        };
        shaper.update();
        shaper
    }
}

impl Effect for Transient {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "attack" => self.attack.set(value / 100.0),
            "sustain" => self.sustain.set(value / 100.0),
            "output" => self.output.set(db_to_gain(value)),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.attack.snap();
        self.sustain.snap();
        self.output.snap();
        for follower in [&mut self.peak, &mut self.slow_peak, &mut self.hold, &mut self.gain] {
            follower.reset();
        }
        self.mean_square = 0.0;
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let attack = self.attack.next(self.smoothing);
            let sustain = self.sustain.next(self.smoothing);
            let shape = self.shape_db(frame[0].abs().max(frame[1].abs()), attack, sustain);
            let gain = db_to_gain(shape) * self.output.next(self.smoothing);
            frame[0] *= gain;
            frame[1] *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, peak, rms, sine, tone};

    fn render(set: &[(&str, f32)], input: &[f32]) -> Vec<f32> {
        let mut shaper = build::<Transient>(set.iter().copied());
        let mut output = input.to_vec();
        shaper.process(&mut output, None);
        output
    }

    /// 100 ms of silence, then a 1 kHz tone that starts at once: a hit with a sustained body.
    fn hit() -> Vec<f32> {
        let mut signal = vec![0.0; SR as usize / 5];
        signal.extend(sine(1_000.0, 0.3, SR as usize / 2));
        signal
    }

    /// A 1 kHz tone that dies away like a drum (time constant 100 ms).
    fn decaying() -> Vec<f32> {
        let mut signal = sine(1_000.0, 0.8, SR as usize / 2);
        for (i, frame) in signal.chunks_exact_mut(2).enumerate() {
            let envelope = (-(i as f32) / (0.1 * SR as f32)).exp();
            frame[0] *= envelope;
            frame[1] *= envelope;
        }
        signal
    }

    /// Stereo frames `from..to` (in ms) of a buffer.
    fn span(buffer: &[f32], from_ms: usize, to_ms: usize) -> &[f32] {
        let frame = |ms: usize| ms * SR as usize / 1_000 * 2;
        &buffer[frame(from_ms)..frame(to_ms)]
    }

    #[test]
    fn neutral_settings_leave_the_sound_alone() {
        let input = hit();
        assert_eq!(render(&[], &input), input);
    }

    #[test]
    fn a_steady_tone_is_left_alone_whatever_the_settings() {
        let input = sine(1_000.0, 0.3, SR as usize);
        for set in [[("attack", 100.0), ("sustain", 100.0)], [("attack", -100.0), ("sustain", -100.0)]] {
            let output = render(&set, &input);
            let settled = &output[SR as usize..];
            assert!((tone(settled, 1_000.0) / 0.3 - 1.0).abs() < 0.01, "{set:?}: {}", tone(settled, 1_000.0));
        }
    }

    #[test]
    fn attack_makes_the_start_of_a_hit_harder_or_softer_and_leaves_its_body() {
        let input = hit();
        let start = peak(span(&input, 100, 110));
        let louder = render(&[("attack", 100.0)], &input);
        assert!(peak(span(&louder, 100, 110)) > start * 1.5, "{}", peak(span(&louder, 100, 110)));
        let softer = render(&[("attack", -100.0)], &input);
        assert!(peak(span(&softer, 100, 110)) < start * 0.75, "{}", peak(span(&softer, 100, 110)));
        assert!((rms(span(&louder, 500, 600)) / rms(span(&input, 500, 600)) - 1.0).abs() < 0.02, "the body is unchanged");
    }

    #[test]
    fn sustain_lengthens_or_shortens_the_tail() {
        let input = decaying();
        let tail = rms(span(&input, 200, 300));
        assert!(rms(span(&render(&[("sustain", 100.0)], &input), 200, 300)) > tail * 2.0);
        assert!(rms(span(&render(&[("sustain", -100.0)], &input), 200, 300)) < tail * 0.5);
    }

    #[test]
    fn turning_the_knobs_does_not_click() {
        let input = sine(1_000.0, 0.3, SR as usize);
        let settled = SR as usize / 5 * 2;
        let mut shaper = build::<Transient>([]);
        let mut signal = input.clone();
        shaper.process(&mut signal[..settled], None);
        for (block, chunk) in signal[settled..].chunks_mut(256).enumerate() {
            shaper.set_param("attack", if block % 2 == 0 { 100.0 } else { -100.0 });
            shaper.set_param("sustain", if block % 3 == 0 { 100.0 } else { -100.0 });
            shaper.process(chunk, None);
        }
        let (after, before) = (max_step(&signal[settled..]), max_step(&input[settled..]));
        assert!(after < before * 1.1, "{after} with {before}");
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut shaper = build::<Transient>([]);
        assert_eq!(shaper.latency_samples(), 0);
        assert_latency_at_impulse(&mut shaper);
    }
}
