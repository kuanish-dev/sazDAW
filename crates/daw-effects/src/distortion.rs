//! Distortion: drive into a waveshaper (soft tanh, hard clip or foldback), then a tone lowpass, dry/wet mix and output.
//! Drive, mix and output are smoothed (no zipper noise on knob moves and automation); a broken input sample
//! (NaN/inf) becomes silence instead of poisoning the filters.
//! The graph in the interface draws its curve by the same shapes and drive/mix/output path
//! (`app/src/effects/shaperCurve.ts`): a change here must be mirrored there.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::biquad::{BUTTERWORTH_Q, Biquad, Coefficients};
use crate::smooth::{Smoothed, smooth_coefficient};
use crate::{EffectKind, clamp_param, db_to_gain};

const SHAPES: [L10n; 3] = [
    L10n { en: "Soft (tanh)", ru: "Мягкая (tanh)" },
    L10n { en: "Hard (clip)", ru: "Жёсткая (клип)" },
    L10n { en: "Foldback", ru: "Фолдбэк" },
];

const PARAMS: [ParamInfo; 5] = [
    ParamInfo {
        name: "drive",
        label: L10n { en: "Drive, dB", ru: "Драйв, дБ" },
        min: 0.0,
        max: 40.0,
        default: 12.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "tone",
        label: L10n { en: "Tone, Hz", ru: "Тон, Гц" },
        min: 200.0,
        max: 16_000.0,
        default: 8_000.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "mix",
        label: L10n { en: "Mix", ru: "Микс" },
        min: 0.0,
        max: 1.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "output",
        label: L10n { en: "Output, dB", ru: "Выход, дБ" },
        min: -24.0,
        max: 6.0,
        default: -6.0,
        kind: ParamKind::Number,
        choices: &[],
        about: L10n::EMPTY,
    },
    ParamInfo {
        name: "shape",
        label: L10n { en: "Shape", ru: "Форма" },
        min: 0.0,
        max: 2.0,
        default: 0.0,
        kind: ParamKind::Choice,
        choices: &SHAPES,
        about: L10n::EMPTY,
    },
];

/// The waveshaper curve, in the order of the `shape` options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Soft,
    Hard,
    Fold,
}

impl Shape {
    fn from_index(value: f32) -> Self {
        match value as u8 {
            0 => Shape::Soft,
            1 => Shape::Hard,
            _ => Shape::Fold,
        }
    }

    /// Always finite and within `[-1, 1]`.
    fn apply(self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        match self {
            Shape::Soft => x.tanh(),
            Shape::Hard => x.clamp(-1.0, 1.0),
            Shape::Fold => {
                let t = (x + 1.0).rem_euclid(4.0);
                if t < 2.0 { t - 1.0 } else { 3.0 - t }
            }
        }
    }
}

pub struct Distortion {
    sample_rate: f32,
    smoothing: f32,
    drive: Smoothed,
    /// Tone cutoff frequency: the filter coefficients are recomputed from it, also when the sample rate changes.
    tone_hz: f32,
    tone: Coefficients,
    mix: Smoothed,
    output: Smoothed,
    shape: Shape,
    filters: [Biquad; 2],
}

impl EffectKind for Distortion {
    const KIND: &'static str = "distortion";
    const LABEL: L10n = L10n { en: "Distortion", ru: "Дисторшн" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let sample_rate = config.sample_rate as f32;
        Self {
            sample_rate,
            smoothing: smooth_coefficient(sample_rate),
            drive: Smoothed::new(1.0),
            tone_hz: 0.0,
            tone: Coefficients::PASS,
            mix: Smoothed::new(1.0),
            output: Smoothed::new(1.0),
            shape: Shape::Soft,
            filters: [Biquad::default(); 2],
        }
    }
}

impl Distortion {
    fn update_tone(&mut self) {
        self.tone = Coefficients::low_pass(self.sample_rate, self.tone_hz, BUTTERWORTH_Q);
    }
}

impl Effect for Distortion {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.smoothing = smooth_coefficient(self.sample_rate);
        self.update_tone();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((_, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        match name {
            "drive" => self.drive.set(db_to_gain(value)),
            "tone" => {
                self.tone_hz = value;
                self.update_tone();
            }
            "mix" => self.mix.set(value),
            "output" => self.output.set(db_to_gain(value)),
            "shape" => self.shape = Shape::from_index(value),
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.drive.snap();
        self.mix.snap();
        self.output.snap();
        for filter in &mut self.filters {
            filter.reset();
        }
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            let drive = self.drive.next(self.smoothing);
            let mix = self.mix.next(self.smoothing);
            let output = self.output.next(self.smoothing);
            for (filter, sample) in self.filters.iter_mut().zip(frame.iter_mut()) {
                let dry = if sample.is_finite() { *sample } else { 0.0 };
                let toned = filter.process(&self.tone, self.shape.apply(dry * drive));
                if !toned.is_finite() {
                    filter.reset();
                }
                let out = (dry + (toned - dry) * mix) * output;
                *sample = if out.is_finite() { out } else { 0.0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{peak, rms, sine};

    const SHAPE_INDICES: [f32; 3] = [0.0, 1.0, 2.0];

    #[test]
    fn output_is_bounded_even_with_huge_input() {
        for shape in SHAPE_INDICES {
            let mut d = build::<Distortion>([("drive", 40.0), ("output", 0.0), ("shape", shape)]);
            let mut signal = sine(100.0, 1.0, 4_800);
            d.process(&mut signal, None);
            assert!(peak(&signal) <= 1.3, "the shaper + filter must not go far beyond 1.0 (shape {shape})");
            assert!(signal.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn silence_stays_silence_for_every_shape() {
        for shape in SHAPE_INDICES {
            let mut d = build::<Distortion>([("drive", 40.0), ("shape", shape)]);
            let mut signal = vec![0.0; 960];
            d.process(&mut signal, None);
            assert!(signal.iter().all(|v| *v == 0.0), "shape {shape}");
        }
    }

    #[test]
    fn drive_flattens_the_wave_and_raises_the_level_of_quiet_input() {
        let quiet = sine(200.0, 0.05, 9_600);
        let mut low = quiet.clone();
        let mut high = quiet.clone();
        build::<Distortion>([("drive", 0.0), ("output", 0.0)]).process(&mut low, None);
        build::<Distortion>([("drive", 30.0), ("output", 0.0)]).process(&mut high, None);
        assert!(rms(&high) > rms(&low) * 5.0);
    }

    #[test]
    fn hard_clip_limits_at_one() {
        assert_eq!(Shape::Hard.apply(100.0), 1.0);
        assert_eq!(Shape::Hard.apply(-100.0), -1.0);
        assert_eq!(Shape::Hard.apply(0.4), 0.4);
    }

    #[test]
    fn foldback_stays_within_one_and_folds_loud_input_back() {
        assert!((-1000..1000).all(|i| (-1.0..=1.0).contains(&Shape::Fold.apply(i as f32 * 0.1))));
        assert!((Shape::Fold.apply(1.5) - 0.5).abs() < 1e-6, "above 1 the wave turns back down");
    }

    #[test]
    fn shapes_sound_different() {
        let render = |shape: f32| {
            let mut d = build::<Distortion>([("drive", 24.0), ("shape", shape), ("tone", 16_000.0)]);
            let mut signal = sine(220.0, 0.5, 4_800);
            d.process(&mut signal, None);
            signal
        };
        assert_ne!(render(0.0), render(1.0));
        assert_ne!(render(1.0), render(2.0));
    }

    #[test]
    fn broken_input_gives_silence_and_does_not_poison_the_filter() {
        for shape in SHAPE_INDICES {
            let mut d = build::<Distortion>([("drive", 20.0), ("shape", shape)]);
            let mut bad = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::NAN];
            d.process(&mut bad, None);
            assert!(bad.iter().all(|v| *v == 0.0), "shape {shape}: {bad:?}");
            let mut after = sine(440.0, 0.3, 960);
            d.process(&mut after, None);
            assert!(after.iter().all(|v| v.is_finite()) && peak(&after) > 0.01, "shape {shape}: plays on after the broken samples");
        }
    }

    /// `prepare` (a device sample rate change) used to reset the tone to 8 kHz although the user had set another one.
    #[test]
    fn prepare_keeps_the_chosen_tone() {
        let render = |prepare: bool| {
            let mut d = build::<Distortion>([("tone", 500.0), ("output", 0.0)]);
            if prepare {
                d.prepare(StreamConfig::DEFAULT);
            }
            let mut signal = sine(6_000.0, 0.5, 9_600);
            d.process(&mut signal, None);
            rms(&signal[4_800..])
        };
        assert!((render(true) - render(false)).abs() < 1e-4, "{} vs {}", render(true), render(false));
    }

    #[test]
    fn zero_mix_keeps_dry_signal_times_output() {
        let mut d = build::<Distortion>([("mix", 0.0), ("output", 0.0)]);
        let original = sine(440.0, 0.3, 480);
        let mut signal = original.clone();
        d.process(&mut signal, None);
        for (a, b) in signal.iter().zip(&original) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn output_change_glides_instead_of_jumping() {
        let mut d = build::<Distortion>([("mix", 0.0), ("output", 0.0)]);
        d.set_param("output", -24.0);
        let mut signal = vec![0.5; 4];
        d.process(&mut signal, None);
        assert!(signal[0] > 0.45, "the first frame after the change is still near the old level: {}", signal[0]);
    }
}
