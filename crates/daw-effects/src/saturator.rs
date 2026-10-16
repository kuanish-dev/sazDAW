//! Saturator: soft (tanh), tube (asymmetric, adds even harmonics) or hard clipping.
//! The nonlinearity runs at twice the sample rate (linear interpolation up, a 4th-order filter and decimation down):
//! this noticeably reduces aliasing from harmonics above Nyquist, though it does not remove it fully, which is enough
//! for a cheap built-in effect.
//! The graph in the interface draws its curve by the same `shapes` and drive/mix/output path
//! (`app/src/domain/shaperCurve.ts`): a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::biquad::{Biquad, Coefficients};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param, db_to_gain};

const TYPES: [L10n; 3] = [L10n { en: "Soft", ru: "Мягкий" }, L10n { en: "Tube", ru: "Ламповый" }, L10n { en: "Hard", ru: "Жёсткий" }];

const PARAMS: [ParamInfo; 4] = [
    ParamInfo {
        name: "drive",
        label: L10n { en: "Drive, dB", ru: "Драйв, дБ" },
        min: 0.0,
        max: 36.0,
        default: 6.0,
        kind: ParamKind::Number,
        choices: &[],
    },
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
        name: "output",
        label: L10n { en: "Output, dB", ru: "Выход, дБ" },
        min: -24.0,
        max: 6.0,
        default: 0.0,
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

/// Offset of the tube type: the tanh curve shifted so the positive half-wave clips earlier than the negative one.
const TUBE_BIAS: f32 = 0.3;
/// Q of the two sections of the 4th-order Butterworth filter (flat passband).
const DECIMATION_Q: [f32; 2] = [0.541_196_1, 1.306_563];
/// Decimation filter cutoff as a fraction of the original sample rate (just below half of it).
const DECIMATION_CUTOFF: f32 = 0.45;
/// DC blocker cutoff, Hz: the tube curve is asymmetric and offsets the signal.
const DC_CUTOFF_HZ: f32 = 10.0;

/// All three clipping curves at once: the type changes by crossfading between them.
fn shapes(x: f32) -> [f32; 3] {
    [x.tanh(), (x + TUBE_BIAS).tanh() - TUBE_BIAS.tanh(), x.clamp(-1.0, 1.0)]
}

#[derive(Clone, Copy, Default)]
struct Channel {
    previous: f32,
    decimation: [Biquad; 2],
    dc_in: f32,
    dc_out: f32,
}

pub struct Saturator {
    sample_rate: f32,
    smoothing: f32,
    decimation: [Coefficients; 2],
    dc_pole: f32,
    drive: Smoothed,
    weights: [Smoothed; 3],
    output: Smoothed,
    mix: Smoothed,
    channels: [Channel; 2],
}

impl Saturator {
    fn update_filters(&mut self) {
        self.smoothing = smooth_coefficient(self.sample_rate);
        let cutoff = self.sample_rate * DECIMATION_CUTOFF;
        self.decimation = DECIMATION_Q.map(|q| Coefficients::low_pass(self.sample_rate * 2.0, cutoff, q));
        self.dc_pole = (-std::f32::consts::TAU * DC_CUTOFF_HZ / self.sample_rate).exp();
    }
}

impl Channel {
    /// One frame of the already amplified signal: two steps at twice the rate, the second goes out, then DC is removed.
    fn process(&mut self, filters: &[Coefficients; 2], weights: &[f32; 3], dc_pole: f32, input: f32) -> f32 {
        let halfway = f32::midpoint(self.previous, input);
        self.previous = input;
        let mut shaped = 0.0;
        for x in [halfway, input] {
            let curve: f32 = shapes(x).iter().zip(weights).map(|(s, w)| s * w).sum();
            let first = self.decimation[0].process(&filters[0], curve);
            shaped = self.decimation[1].process(&filters[1], first);
        }
        let out = shaped - self.dc_in + dc_pole * self.dc_out;
        self.dc_in = shaped;
        self.dc_out = out;
        out
    }
}

impl EffectKind for Saturator {
    const KIND: &'static str = "saturator";
    const LABEL: L10n = L10n { en: "Saturator", ru: "Сатуратор" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut saturator = Self {
            sample_rate: config.sample_rate as f32,
            smoothing: 0.0,
            decimation: [Coefficients::PASS; 2],
            dc_pole: 0.0,
            drive: Smoothed::new(1.0),
            weights: [Smoothed::new(1.0), Smoothed::new(0.0), Smoothed::new(0.0)],
            output: Smoothed::new(1.0),
            mix: Smoothed::new(1.0),
            channels: [Channel::default(); 2],
        };
        saturator.update_filters();
        saturator
    }
}

impl Effect for Saturator {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update_filters();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "drive" => self.drive.set(db_to_gain(value)),
            "type" => {
                for (index, weight) in self.weights.iter_mut().enumerate() {
                    weight.set(if index == value as usize { 1.0 } else { 0.0 });
                }
            }
            "output" => self.output.set(db_to_gain(value)),
            "mix" => self.mix.set(value),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.drive.snap();
        self.weights.iter_mut().for_each(Smoothed::snap);
        self.output.snap();
        self.mix.snap();
        self.channels = [Channel::default(); 2];
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let drive = self.drive.next(self.smoothing);
            let weights = self.weights.each_mut().map(|w| w.next(self.smoothing));
            let output = self.output.next(self.smoothing);
            let mix = self.mix.next(self.smoothing);
            for (channel, sample) in self.channels.iter_mut().zip(frame.iter_mut()) {
                let dry = *sample;
                let wet = channel.process(&self.decimation, &weights, self.dc_pole, dry * drive);
                *sample = (dry + (wet - dry) * mix) * output;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{SR, max_step, peak, sine, tone};

    /// A sine of `hz` through the saturator; returns the settled part.
    fn render(set: &[(&str, f32)], hz: f32, amplitude: f32) -> Vec<f32> {
        let mut s = build::<Saturator>(set.iter().copied());
        let mut signal = sine(hz, amplitude, SR as usize);
        s.process(&mut signal, None);
        signal.split_off(SR as usize)
    }

    #[test]
    fn output_is_bounded_for_every_type() {
        for kind in 0..3 {
            let out = render(&[("type", kind as f32), ("drive", 36.0)], 100.0, 1.0);
            assert!(out.iter().all(|v| v.is_finite()));
            assert!(peak(&out) < 1.5, "type {kind}: {}", peak(&out));
        }
    }

    #[test]
    fn soft_adds_odd_harmonics_and_tube_adds_even_ones() {
        let soft = render(&[("type", 0.0), ("drive", 12.0)], 500.0, 0.5);
        let fundamental = tone(&soft, 500.0);
        assert!(tone(&soft, 1_500.0) > fundamental * 0.05, "3rd harmonic");
        assert!(tone(&soft, 1_000.0) < fundamental * 0.001, "a symmetric curve has no even harmonics");

        let tube = render(&[("type", 1.0), ("drive", 12.0)], 500.0, 0.5);
        assert!(tone(&tube, 1_000.0) > tone(&tube, 500.0) * 0.02, "2nd harmonic");
        assert!(tone(&tube, 0.0).abs() < 0.01, "DC is removed");
    }

    #[test]
    fn quiet_signal_with_zero_drive_passes_almost_unchanged() {
        let out = render(&[("type", 2.0), ("drive", 0.0)], 1_000.0, 0.5);
        assert!((tone(&out, 1_000.0) / 0.5 - 1.0).abs() < 0.01, "{}", tone(&out, 1_000.0));
    }

    /// Hard clipping of 7 kHz: without oversampling the 5th harmonic (35 kHz) would fold to 13 kHz almost fully
    /// (~1/5 of the fundamental); at twice the rate about 1 % remains.
    #[test]
    fn oversampling_reduces_aliasing() {
        let out = render(&[("type", 2.0), ("drive", 24.0)], 7_000.0, 0.5);
        let alias = tone(&out, 13_000.0) / tone(&out, 7_000.0);
        assert!(alias < 0.03, "{alias}");
    }

    /// Changing type, drive and output block by block adds no steps beyond those of the curve itself.
    #[test]
    fn parameter_changes_do_not_click() {
        let baseline = [0.0, 2.0].map(|kind| max_step(&render(&[("type", kind), ("drive", 12.0)], 100.0, 0.5)));
        let baseline = baseline[0].max(baseline[1]);
        let mut s = build::<Saturator>([("drive", 12.0)]);
        let mut signal = sine(100.0, 0.5, SR as usize);
        for (block, chunk) in signal.chunks_mut(256).enumerate() {
            s.set_param("type", if block % 2 == 0 { 0.0 } else { 2.0 });
            s.set_param("output", if block % 3 == 0 { 0.0 } else { -12.0 });
            s.process(chunk, None);
        }
        assert!(max_step(&signal) < baseline * 1.5, "{} with {baseline}", max_step(&signal));
    }
}
