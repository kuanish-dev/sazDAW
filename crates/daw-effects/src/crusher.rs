//! Bitcrusher: lo-fi grit from two kinds of damage. **Bits** rounds every sample to a coarse grid (`2^(bits-1)` steps
//! each side of zero; the knob is continuous, so automation sweeps the grid without jumps of the setting), the hiss and
//! fuzz of an old sampler. **Downsample** holds each sample for that many frames (a fraction is allowed: the hold
//! follows a phase, so 2.5 alternates 2 and 3), the metallic aliasing of a low sample rate; both channels are held at
//! the same moments, so the stereo image stays put. `mix` blends the crushed sound with the clean one and `output` trims
//! the result; both glide (`smooth.rs`). No latency.
//!
//! The graph in the interface draws the staircase curve (`app/src/effects/shaperCurve.ts: crusherTransfer`); the
//! downsampling acts over time and shows only in the live trace: a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, StreamConfig};

use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param, db_to_gain, number};

const PARAMS: [ParamInfo; 4] = [
    number("bits", L10n { en: "Bits", ru: "Биты" }, 1.0, 16.0, 8.0),
    number("downsample", L10n { en: "Downsample, x", ru: "Понижение частоты, x" }, 1.0, 50.0, 1.0),
    number("mix", L10n { en: "Mix", ru: "Смесь" }, 0.0, 1.0, 1.0),
    number("output", L10n { en: "Output, dB", ru: "Выход, дБ" }, -24.0, 6.0, 0.0),
];

/// Rounds `x` to a grid of `steps` per unit: 1 bit keeps −1, 0 and 1, 16 bits is almost transparent.
fn quantize(x: f32, steps: f32) -> f32 {
    (x * steps).round() / steps
}

pub struct Crusher {
    smoothing: f32,
    /// Grid steps per unit of level: `2^(bits-1)`.
    steps: f32,
    /// How many frames each held sample lasts.
    hold: f32,
    mix: Smoothed,
    output: Smoothed,
    /// Frames since the held sample was taken, against `hold`.
    phase: f32,
    held: [f32; 2],
}

impl EffectKind for Crusher {
    const KIND: &'static str = "crusher";
    const LABEL: L10n = L10n { en: "Bitcrusher", ru: "Биткрашер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        Self {
            smoothing: smooth_coefficient(config.sample_rate as f32),
            steps: 128.0,
            hold: 1.0,
            mix: Smoothed::new(1.0),
            output: Smoothed::new(1.0),
            phase: 0.0,
            held: [0.0; 2],
        }
    }
}

impl Effect for Crusher {
    fn prepare(&mut self, config: StreamConfig) {
        self.smoothing = smooth_coefficient(config.sample_rate as f32);
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "bits" => self.steps = (value - 1.0).exp2(),
            "downsample" => self.hold = value,
            "mix" => self.mix.set(value),
            "output" => self.output.set(db_to_gain(value)),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.mix.snap();
        self.output.snap();
        self.phase = self.hold;
        self.held = [0.0; 2];
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let mix = self.mix.next(self.smoothing);
            let output = self.output.next(self.smoothing);
            if self.phase >= self.hold {
                self.phase -= self.hold;
                self.held = [frame[0], frame[1]];
            }
            self.phase += 1.0;
            for (sample, held) in frame.iter_mut().zip(self.held) {
                let crushed = quantize(held, self.steps);
                *sample = (*sample * (1.0 - mix) + crushed * mix) * output;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, assert_latency_at_impulse, max_step, sine, tone};

    fn render(set: &[(&str, f32)], hz: f32, amplitude: f32) -> Vec<f32> {
        let mut crusher = build::<Crusher>(set.iter().copied());
        let mut signal = sine(hz, amplitude, SR as usize);
        crusher.process(&mut signal, None);
        signal
    }

    #[test]
    fn sixteen_bits_without_downsampling_is_almost_transparent() {
        let clean = sine(1_000.0, 0.8, SR as usize);
        let out = render(&[("bits", 16.0)], 1_000.0, 0.8);
        assert!(out.iter().zip(&clean).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn bits_put_every_sample_on_a_coarse_grid() {
        let out = render(&[("bits", 4.0)], 440.0, 0.9);
        assert!(out.iter().all(|v| ((v * 8.0) - (v * 8.0).round()).abs() < 1e-5), "4 bits: steps of 1/8");
        assert!(tone(&out, 1_320.0) > 0.01, "the grid adds odd harmonics");
    }

    #[test]
    fn downsampling_holds_each_sample_for_the_set_number_of_frames() {
        let out = render(&[("bits", 16.0), ("downsample", 4.0)], 440.0, 0.9);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let changes = left.windows(2).filter(|w| w[0] != w[1]).count();
        assert!((changes as f32 / left.len() as f32 - 0.25).abs() < 0.01, "a new value every fourth frame: {changes}");
        assert!(out.chunks_exact(2).all(|f| f[0] == f[1]), "both channels are held together");
    }

    #[test]
    fn mix_at_zero_is_the_clean_sound() {
        let clean = sine(300.0, 0.7, SR as usize);
        let out = render(&[("bits", 2.0), ("downsample", 20.0), ("mix", 0.0)], 300.0, 0.7);
        assert!(out.iter().zip(&clean).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn mix_and_output_glide_without_clicks() {
        let baseline = max_step(&render(&[("bits", 16.0)], 200.0, 0.5)[SR as usize..]);
        let mut crusher = build::<Crusher>([("bits", 16.0)]);
        let mut signal = sine(200.0, 0.5, SR as usize);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            crusher.set_param("mix", if block % 2 == 0 { 0.0 } else { 1.0 });
            crusher.set_param("output", if block % 3 == 0 { 0.0 } else { -6.0 });
            crusher.process(chunk, None);
        }
        assert!(max_step(&signal[SR as usize..]) < baseline * 1.5, "{} with {baseline}", max_step(&signal[SR as usize..]));
    }

    #[test]
    fn reports_no_latency_and_passes_an_impulse_at_once() {
        let mut crusher = build::<Crusher>([("bits", 16.0)]);
        assert_eq!(crusher.latency_samples(), 0);
        assert_latency_at_impulse(&mut crusher);
    }
}
