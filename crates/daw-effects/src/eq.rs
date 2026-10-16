//! Equalizer: low cut (high-pass), low shelf, peaking, high shelf, high cut (low-pass). The cuts are 12 dB/oct
//! Butterworth biquads; a cut at its extreme frequency (`low_cut` at its minimum, `high_cut` at its maximum) is off, so a
//! new EQ changes nothing. The interface draws its response curve by the same formulas (`app/src/domain/eqCurve.ts`).
//! The linear-phase EQ (`linear_eq`) has the same table and bands: it takes their magnitude from `filters`.

use daw_contracts::{Effect, L10n, ParamInfo, ParamKind, StreamConfig};

use crate::biquad::{BUTTERWORTH_Q, Biquad, Coefficients};
use crate::{EffectKind, clamp_param};

/// `low_cut` at this frequency (its minimum) and `high_cut` at `HIGH_CUT_OFF` (its maximum) are off.
const LOW_CUT_OFF: f32 = 20.0;
const HIGH_CUT_OFF: f32 = 20_000.0;

/// The params of both EQs (this one and the linear-phase one).
pub(crate) const PARAMS: [ParamInfo; VALUES] = [
    ParamInfo {
        name: "low_cut",
        label: L10n { en: "Low cut, Hz", ru: "Срез низа, Гц" },
        min: LOW_CUT_OFF,
        max: 1_000.0,
        default: LOW_CUT_OFF,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "low_gain",
        label: L10n { en: "Low, dB", ru: "Низ, дБ" },
        min: -18.0,
        max: 18.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "low_freq",
        label: L10n { en: "Low, Hz", ru: "Низ, Гц" },
        min: 40.0,
        max: 400.0,
        default: 100.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mid_gain",
        label: L10n { en: "Mid, dB", ru: "Середина, дБ" },
        min: -18.0,
        max: 18.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mid_freq",
        label: L10n { en: "Mid, Hz", ru: "Середина, Гц" },
        min: 200.0,
        max: 8_000.0,
        default: 1_000.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "mid_q",
        label: L10n { en: "Mid Q", ru: "Середина Q" },
        min: 0.3,
        max: 8.0,
        default: 1.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "high_gain",
        label: L10n { en: "High, dB", ru: "Верх, дБ" },
        min: -18.0,
        max: 18.0,
        default: 0.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "high_freq",
        label: L10n { en: "High, Hz", ru: "Верх, Гц" },
        min: 2_000.0,
        max: 16_000.0,
        default: 8_000.0,
        kind: ParamKind::Number,
        choices: &[],
    },
    ParamInfo {
        name: "high_cut",
        label: L10n { en: "High cut, Hz", ru: "Срез верха, Гц" },
        min: 1_000.0,
        max: HIGH_CUT_OFF,
        default: HIGH_CUT_OFF,
        kind: ParamKind::Number,
        choices: &[],
    },
];

/// How many params (values) and bands the EQ has.
pub(crate) const VALUES: usize = 9;
const BANDS: usize = 5;

pub struct Eq {
    sample_rate: f32,
    values: [f32; VALUES],
    coefficients: [Coefficients; BANDS],
    state: [[Biquad; BANDS]; 2],
}

/// The bands' filters for the param values in table order.
pub(crate) fn filters(sample_rate: f32, values: &[f32; VALUES]) -> [Coefficients; BANDS] {
    let [low_cut, low_gain, low_freq, mid_gain, mid_freq, mid_q, high_gain, high_freq, high_cut] = *values;
    let sr = sample_rate;
    [
        if low_cut <= LOW_CUT_OFF { Coefficients::PASS } else { Coefficients::high_pass(sr, low_cut, BUTTERWORTH_Q) },
        Coefficients::low_shelf(sr, low_freq, low_gain),
        Coefficients::peaking(sr, mid_freq, mid_gain, mid_q),
        Coefficients::high_shelf(sr, high_freq, high_gain),
        if high_cut >= HIGH_CUT_OFF { Coefficients::PASS } else { Coefficients::low_pass(sr, high_cut, BUTTERWORTH_Q) },
    ]
}

impl Eq {
    fn update(&mut self) {
        self.coefficients = filters(self.sample_rate, &self.values);
    }
}

impl EffectKind for Eq {
    const KIND: &'static str = "eq";
    const LABEL: L10n = L10n { en: "EQ", ru: "Эквалайзер" };
    const PARAMS: &'static [ParamInfo] = &PARAMS;

    fn new(config: StreamConfig) -> Self {
        let mut eq = Self {
            sample_rate: config.sample_rate as f32,
            values: PARAMS.map(|p| p.default),
            coefficients: [Coefficients::PASS; BANDS],
            state: [[Biquad::default(); BANDS]; 2],
        };
        eq.update();
        eq
    }
}

impl Effect for Eq {
    fn prepare(&mut self, config: StreamConfig) {
        self.sample_rate = config.sample_rate as f32;
        self.update();
        self.reset();
    }

    fn set_param(&mut self, name: &str, value: f32) {
        let Some((index, value)) = clamp_param(&PARAMS, name, value) else {
            return;
        };
        self.values[index] = value;
        self.update();
    }

    fn reset(&mut self) {
        for channel in &mut self.state {
            for band in channel {
                band.reset();
            }
        }
    }

    fn process(&mut self, interleaved: &mut [f32], _sidechain: Option<&[f32]>) {
        for frame in interleaved.chunks_exact_mut(2) {
            for (channel, sample) in frame.iter_mut().enumerate() {
                let mut value = *sample;
                for (band, coefficients) in self.state[channel].iter_mut().zip(&self.coefficients) {
                    value = band.process(coefficients, value);
                }
                *sample = value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build;
    use crate::testing::{rms, sine};

    fn run(set: &[(&str, f32)], hz: f32) -> (f32, f32) {
        let mut eq = build::<Eq>(set.iter().copied());
        let input = sine(hz, 0.5, 48_000);
        let mut output = input.clone();
        eq.process(&mut output, None);
        // Skip the transient.
        (rms(&input[24_000..]), rms(&output[24_000..]))
    }

    #[test]
    fn flat_settings_do_not_change_the_signal() {
        let (before, after) = run(&[], 1_000.0);
        assert!((before - after).abs() < 0.002, "{before} vs {after}");
    }

    #[test]
    fn shelves_and_peak_boost_and_cut_their_bands() {
        let (before, after) = run(&[("low_gain", 12.0)], 60.0);
        assert!(after > before * 2.5, "the low end must be boosted: {before} → {after}");
        let (before, after) = run(&[("low_gain", 12.0)], 6_000.0);
        assert!((after / before - 1.0).abs() < 0.1, "the high end must not change");
        let (before, after) = run(&[("mid_gain", -12.0)], 1_000.0);
        assert!(after < before * 0.35);
        let (before, after) = run(&[("high_gain", 12.0)], 12_000.0);
        assert!(after > before * 2.5);
    }

    #[test]
    fn cuts_are_off_at_their_extremes_and_roll_off_12_db_per_octave() {
        let (before, after) = run(&[("low_cut", LOW_CUT_OFF), ("high_cut", HIGH_CUT_OFF)], 30.0);
        assert!((after / before - 1.0).abs() < 0.02, "off: {before} → {after}");

        // An octave below the cutoff a 12 dB/oct high-pass is about −12 dB (×0.25); at the cutoff −3 dB (×0.71).
        let (before, after) = run(&[("low_cut", 400.0)], 200.0);
        assert!((after / before - 0.25).abs() < 0.05, "an octave below: {}", after / before);
        let (before, after) = run(&[("low_cut", 400.0)], 400.0);
        assert!((after / before - 0.707).abs() < 0.05, "at the cutoff: {}", after / before);
        let (before, after) = run(&[("low_cut", 400.0)], 4_000.0);
        assert!((after / before - 1.0).abs() < 0.03, "the highs pass");

        let (before, after) = run(&[("high_cut", 2_000.0)], 4_000.0);
        assert!((after / before - 0.25).abs() < 0.05, "an octave above: {}", after / before);
        let (before, after) = run(&[("high_cut", 2_000.0)], 200.0);
        assert!((after / before - 1.0).abs() < 0.03, "the lows pass");
    }
}
