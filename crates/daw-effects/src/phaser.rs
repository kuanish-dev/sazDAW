//! Phaser: the swirl of a chain of first-order all-pass filters mixed with the dry sound. Each stage shifts the phase
//! by 0° at the lows to 180° at the top (90° at its corner) without changing the level; where the chain's total shift
//! reaches 180°, 540°… the wet sound cancels the dry one and a notch appears (`stages` of 4 give two notches, 12 give
//! six). An LFO (`rate`, a sine) sweeps the corner of every stage between `low` and `high` evenly in octaves, so the
//! notches glide up and down; `feedback` sends the chain's output back into its input, sharpening the notches and
//! adding peaks between them (negative feedback moves the peaks). `spread` starts the right channel's LFO that many
//! degrees later, so the notches move through the stereo field. `mix` 0.5 gives the deepest notches.
//!
//! The corners glide in octaves and `feedback` and `mix` glide too (`smooth.rs`); a change of `stages` clears the
//! stages it brings in. No latency.
//!
//! The graph in the interface draws the chain's response with the LFO turning at the same rate
//! (`app/src/effects/phaserCurve.ts`): a change here must be mirrored there.

use std::f32::consts::{PI, TAU};

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, choice, clamp_param, number};

/// How many stages each option of `stages` has.
const STAGES: [usize; 4] = [4, 6, 8, 12];
const MAX_STAGES: usize = 12;

const PARAMS: [ParamInfo; 7] = [
    number("rate", L10n { en: "Rate, Hz", ru: "Скорость, Гц" }, 0.02, 10.0, 0.4),
    number("low", L10n { en: "Low, Hz", ru: "Низ, Гц" }, 40.0, 4_000.0, 200.0),
    number("high", L10n { en: "High, Hz", ru: "Верх, Гц" }, 200.0, 16_000.0, 3_000.0),
    choice("stages", L10n { en: "Stages", ru: "Ступени" }, &[L10n::same("4"), L10n::same("6"), L10n::same("8"), L10n::same("12")], 1.0),
    number("feedback", L10n { en: "Feedback", ru: "Обратная связь" }, -0.9, 0.9, 0.5),
    number("spread", L10n { en: "Stereo spread, °", ru: "Стерео-разнос, °" }, 0.0, 180.0, 90.0),
    number("mix", L10n { en: "Mix", ru: "Смесь" }, 0.0, 1.0, 0.5),
];

/// The all-pass coefficient for a corner at `hz`: `(tan(πf/fs) − 1) / (tan(πf/fs) + 1)`.
fn coefficient(hz: f32, sample_rate: f32) -> f32 {
    let t = (PI * hz.min(0.45 * sample_rate) / sample_rate).tan();
    (t - 1.0) / (t + 1.0)
}

pub struct Phaser {
    sample_rate: f32,
    smoothing: f32,
    rate_hz: f32,
    /// The sweep's ends in octaves (log2 of hertz): equal knob moves sound equal at any height.
    low_octaves: Smoothed,
    high_octaves: Smoothed,
    feedback: Smoothed,
    mix: Smoothed,
    /// The right channel's LFO offset, in cycles.
    spread: f32,
    stages: usize,
    /// The LFO's position, `0..1`.
    phase: f32,
    /// Per channel and stage: the stage's last input and output.
    state: [[[f32; 2]; MAX_STAGES]; 2],
    /// Per channel: the chain's last output, fed back into its input.
    last: [f32; 2],
}

impl EffectKind for Phaser {
    const KIND: &'static str = "phaser";
    const LABEL: L10n = L10n { en: "Phaser", ru: "Фейзер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        Self {
            sample_rate,
            smoothing: smooth_coefficient(sample_rate),
            rate_hz: 0.4,
            low_octaves: Smoothed::new(200f32.log2()),
            high_octaves: Smoothed::new(3_000f32.log2()),
            feedback: Smoothed::new(0.0),
            mix: Smoothed::new(0.5),
            spread: 0.25,
            stages: STAGES[1],
            phase: 0.0,
            state: [[[0.0; 2]; MAX_STAGES]; 2],
            last: [0.0; 2],
        }
    }
}

impl Effect for Phaser {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.smoothing = smooth_coefficient(self.sample_rate);
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "rate" => self.rate_hz = value,
            "low" => self.low_octaves.set(value.log2()),
            "high" => self.high_octaves.set(value.log2()),
            "stages" => {
                let stages = STAGES[value as usize];
                // Stages coming in start from silence, not from what they held when they last played.
                for channel in &mut self.state {
                    for stage in &mut channel[self.stages.min(stages)..stages] {
                        *stage = [0.0; 2];
                    }
                }
                self.stages = stages;
            }
            "feedback" => self.feedback.set(value),
            "spread" => self.spread = value / 360.0,
            "mix" => self.mix.set(value),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.low_octaves.snap();
        self.high_octaves.snap();
        self.feedback.snap();
        self.mix.snap();
        self.phase = 0.0;
        self.state = [[[0.0; 2]; MAX_STAGES]; 2];
        self.last = [0.0; 2];
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        let step = self.rate_hz / self.sample_rate;
        for frame in interleaved.chunks_exact_mut(2) {
            let low = self.low_octaves.next(self.smoothing);
            let high = self.high_octaves.next(self.smoothing);
            let feedback = self.feedback.next(self.smoothing);
            let mix = self.mix.next(self.smoothing);
            for (c, sample) in frame.iter_mut().enumerate() {
                // 0 at the low end, 1 at the high end; the right channel `spread` later.
                let sweep = 0.5 - 0.5 * (TAU * (self.phase + c as f32 * self.spread)).cos();
                let a = coefficient((low + sweep * (high - low)).exp2(), self.sample_rate);
                let mut x = *sample + feedback * self.last[c];
                for stage in &mut self.state[c][..self.stages] {
                    let y = a * x + stage[0] - a * stage[1];
                    *stage = [x, y];
                    x = y;
                }
                self.last[c] = x;
                *sample = *sample * (1.0 - mix) + x * mix;
            }
            self.phase = (self.phase + step).fract();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, sine, tone};

    /// A sine of `hz` through the phaser; the settled second half.
    fn render(set: &[(&str, f32)], hz: f32) -> Vec<f32> {
        let mut phaser = build::<Phaser>(set.iter().copied());
        let mut signal = sine(hz, 0.5, SR as usize);
        phaser.process(&mut signal, None);
        signal.split_off(SR as usize)
    }

    /// The sweep held at 1 kHz with four stages and no feedback: notches at 0.414 and 2.414 times the corner.
    const STILL: [(&str, f32); 5] = [("low", 1_000.0), ("high", 1_000.0), ("stages", 0.0), ("feedback", 0.0), ("mix", 0.5)];

    #[test]
    fn four_stages_cut_notches_where_the_chain_turns_half_a_cycle() {
        assert!(tone(&render(&STILL, 415.0), 415.0) < 0.5 * 0.02, "the first notch");
        assert!(tone(&render(&STILL, 2_414.0), 2_414.0) < 0.5 * 0.03, "the second notch");
        assert!((tone(&render(&STILL, 1_000.0), 1_000.0) - 0.5).abs() < 0.01, "in phase at the corner: the level is kept");
        assert!(tone(&render(&STILL, 50.0), 50.0) > 0.5 * 0.95, "far below the notches nothing changes");
    }

    #[test]
    fn feedback_lifts_the_peak_between_the_notches() {
        let mut set = STILL;
        set[3] = ("feedback", 0.7);
        assert!(tone(&render(&set, 1_000.0), 1_000.0) > 0.5 * 2.0, "{}", tone(&render(&set, 1_000.0), 1_000.0));
    }

    #[test]
    fn mix_at_zero_is_the_clean_sound() {
        let clean = sine(700.0, 0.5, SR as usize).split_off(SR as usize);
        let out = render(&[("mix", 0.0)], 700.0);
        assert!(out.iter().zip(&clean).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn spread_moves_the_two_channels_apart() {
        let apart = render(&[("rate", 2.0), ("spread", 180.0)], 800.0);
        assert!(apart.chunks_exact(2).any(|f| (f[0] - f[1]).abs() > 0.05), "the notches sweep each channel at its own moment");
        let together = render(&[("rate", 2.0), ("spread", 0.0)], 800.0);
        assert!(together.chunks_exact(2).all(|f| (f[0] - f[1]).abs() < 1e-6));
    }

    #[test]
    fn changing_params_between_blocks_does_not_click() {
        let baseline = max_step(&render(&[], 200.0));
        let mut phaser = build::<Phaser>([]);
        let mut signal = sine(200.0, 0.5, SR as usize);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            phaser.set_param("mix", if block % 2 == 0 { 0.2 } else { 0.8 });
            phaser.set_param("feedback", if block % 3 == 0 { -0.5 } else { 0.5 });
            phaser.set_param("low", if block % 5 == 0 { 100.0 } else { 800.0 });
            phaser.process(chunk, None);
        }
        assert!(max_step(&signal[SR as usize..]) < baseline * 2.5, "{} with {baseline}", max_step(&signal[SR as usize..]));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut phaser = build::<Phaser>([("feedback", 0.0)]);
        assert_eq!(phaser.latency_samples(), 0);
        assert_latency_at_impulse(&mut phaser);
    }
}
