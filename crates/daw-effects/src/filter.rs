//! Filter: low-pass, high-pass or band-pass with resonance, a state variable filter (SVF, Simper's trapezoidal design).
//! Why an SVF rather than a biquad: it stays stable under fast cutoff changes (automation, sweeps)
//! and at high Q, and all three outputs come at once, so a type change is a crossfade between them.
//! The graph in the interface draws its response from the same design (`app/src/domain/filterCurve.ts`): a change here
//! must be mirrored there.

use std::f32::consts::PI;

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param};

const TYPES: [L10n; 3] = [L10n { en: "Low-pass", ru: "НЧ" }, L10n { en: "High-pass", ru: "ВЧ" }, L10n { en: "Band-pass", ru: "Полоса" }];

const PARAMS: [ParamInfo; 4] = [
    ParamInfo {
        name: "type",
        label: L10n { en: "Type", ru: "Тип" },
        min: 0.0,
        max: 2.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &TYPES,
    },
    ParamInfo {
        name: "cutoff",
        label: L10n { en: "Cutoff, Hz", ru: "Срез, Гц" },
        min: 20.0,
        max: 20_000.0,
        default: 1_000.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "resonance",
        label: L10n { en: "Resonance, Q", ru: "Резонанс, Q" },
        min: 0.5,
        max: 12.0,
        default: 0.7,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mix",
        label: L10n { en: "Mix", ru: "Микс" },
        min: 0.0,
        max: 1.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
    },
];

/// State of one SVF channel (two integrators).
#[derive(Clone, Copy, Default)]
struct Channel {
    ic1: f32,
    ic2: f32,
}

/// SVF coefficients for the current cutoff and Q.
#[derive(Clone, Copy, Default)]
struct Coefficients {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Coefficients {
    fn new(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        // Above 0.45 of the sample rate tan goes to infinity: the cutoff is inaudible there anyway, so it is clamped.
        let g = (PI * cutoff.min(sample_rate * 0.45) / sample_rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self { k, a1, a2, a3: g * a2 }
    }
}

impl Channel {
    /// Low-pass, high-pass and band-pass outputs (band-pass is normalized: 0 dB at the cutoff for any Q).
    fn process(&mut self, c: &Coefficients, input: f32) -> [f32; 3] {
        let v3 = input - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        [v2, input - c.k * v1 - v2, c.k * v1]
    }
}

pub struct Filter {
    sample_rate: f32,
    smoothing: f32,
    /// The cutoff is smoothed in log scale: a sweep moves evenly through the octaves.
    log_cutoff: Smoothed,
    q: Smoothed,
    /// Shares of the low-pass, high-pass and band-pass outputs: the type changes by a crossfade, without a click.
    weights: [Smoothed; 3],
    mix: Smoothed,
    coefficients: Coefficients,
    channels: [Channel; 2],
}

impl Filter {
    fn update_coefficients(&mut self) {
        self.coefficients = Coefficients::new(self.sample_rate, self.log_cutoff.next(self.smoothing).exp(), self.q.next(self.smoothing));
    }
}

impl EffectKind for Filter {
    const KIND: &'static str = "filter";
    const LABEL: L10n = L10n { en: "Filter", ru: "Фильтр" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        Self {
            sample_rate,
            smoothing: smooth_coefficient(sample_rate),
            log_cutoff: Smoothed::new(1_000f32.ln()),
            q: Smoothed::new(1.0),
            weights: [Smoothed::new(1.0), Smoothed::new(0.0), Smoothed::new(0.0)],
            mix: Smoothed::new(1.0),
            coefficients: Coefficients::default(),
            channels: [Channel::default(); 2],
        }
    }
}

impl Effect for Filter {
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
            "type" => {
                for (index, weight) in self.weights.iter_mut().enumerate() {
                    weight.set(if index == value as usize { 1.0 } else { 0.0 });
                }
            }
            "cutoff" => self.log_cutoff.set(value.ln()),
            "resonance" => self.q.set(value),
            "mix" => self.mix.set(value),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.log_cutoff.snap();
        self.q.snap();
        self.weights.iter_mut().for_each(Smoothed::snap);
        self.mix.snap();
        self.channels = [Channel::default(); 2];
        self.update_coefficients();
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            // tan is expensive: coefficients are recomputed only while the cutoff or Q is still moving to its target.
            if !(self.log_cutoff.is_settled() && self.q.is_settled()) {
                self.update_coefficients();
            }
            let weights = self.weights.each_mut().map(|w| w.next(self.smoothing));
            let mix = self.mix.next(self.smoothing);
            for (channel, sample) in self.channels.iter_mut().zip(frame.iter_mut()) {
                let outputs = channel.process(&self.coefficients, *sample);
                let wet: f32 = outputs.iter().zip(&weights).map(|(o, w)| o * w).sum();
                *sample += (wet - *sample) * mix;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{max_step, peak, rms, sine};

    /// How many times the filter changes the level of a sine of `hz` (after the transient).
    fn gain(set: &[(&str, f32)], hz: f32) -> f32 {
        let mut f = build::<Filter>(set.iter().copied());
        let input = sine(hz, 0.5, 24_000);
        let mut output = input.clone();
        f.process(&mut output, None);
        rms(&output[12_000..]) / rms(&input[12_000..])
    }

    #[test]
    fn low_pass_keeps_lows_and_cuts_highs() {
        let low_pass = [("type", 0.0), ("cutoff", 500.0)];
        assert!(gain(&low_pass, 10_000.0) < 0.01, "10 kHz must be removed: {}", gain(&low_pass, 10_000.0));
        assert!((gain(&low_pass, 50.0) - 1.0).abs() < 0.02);
    }

    #[test]
    fn high_pass_keeps_highs_and_cuts_lows() {
        let high_pass = [("type", 1.0), ("cutoff", 500.0)];
        assert!(gain(&high_pass, 50.0) < 0.02, "{}", gain(&high_pass, 50.0));
        assert!((gain(&high_pass, 10_000.0) - 1.0).abs() < 0.02);
    }

    #[test]
    fn band_pass_keeps_only_the_band_around_the_cutoff() {
        let band = [("type", 2.0), ("cutoff", 1_000.0), ("resonance", 4.0)];
        assert!((gain(&band, 1_000.0) - 1.0).abs() < 0.05, "{}", gain(&band, 1_000.0));
        assert!(gain(&band, 100.0) < 0.05);
        assert!(gain(&band, 10_000.0) < 0.05);
    }

    #[test]
    fn resonance_boosts_the_cutoff() {
        let flat = gain(&[("cutoff", 1_000.0), ("resonance", 0.7)], 1_000.0);
        let resonant = gain(&[("cutoff", 1_000.0), ("resonance", 8.0)], 1_000.0);
        assert!(resonant > flat * 8.0, "{flat} → {resonant}");
    }

    #[test]
    fn zero_mix_is_dry() {
        let original = sine(10_000.0, 0.5, 2_000);
        let mut signal = original.clone();
        build::<Filter>([("mix", 0.0)]).process(&mut signal, None);
        assert_eq!(signal, original);
    }

    /// Maximum resonance and a cutoff jumping edge to edge every 64 frames: the sound stays finite and bounded.
    #[test]
    fn stays_stable_at_high_resonance_under_fast_automation() {
        for kind in 0..3 {
            let mut f = build::<Filter>([("type", kind as f32), ("resonance", 12.0)]);
            let mut signal = sine(700.0, 1.0, 96_000);
            for (block, chunk) in signal.chunks_mut(128).enumerate() {
                f.set_param("cutoff", if block % 2 == 0 { 20.0 } else { 20_000.0 });
                f.process(chunk, None);
            }
            assert!(signal.iter().all(|v| v.is_finite()), "type {kind}");
            assert!(peak(&signal) < 30.0, "type {kind}: {}", peak(&signal));
        }
    }

    /// Changing type and cutoff mid-sound gives no steps: the step between frames is no larger than the sine's own.
    #[test]
    fn parameter_changes_do_not_click() {
        let mut f = build::<Filter>([("cutoff", 2_000.0)]);
        let mut signal = sine(100.0, 0.5, 48_000);
        let sine_step = max_step(&signal);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            match block {
                40 => f.set_param("type", 1.0),
                80 => f.set_param("cutoff", 50.0),
                120 => f.set_param("type", 2.0),
                _ => {}
            }
            f.process(chunk, None);
        }
        assert!(max_step(&signal) < sine_step * 1.5, "{} with {sine_step}", max_step(&signal));
    }
}
