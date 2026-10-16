//! Linkwitz–Riley crossover: splits a sound into `BANDS` bands that sum back to it with a flat magnitude. Each split is
//! 4th order (24 dB/oct): two Butterworth biquads in a row for the low side and two for the high side, so both sides are
//! −6 dB at the split and in phase. Their sum is not the input but the split's all-pass (`Coefficients::all_pass` at
//! Butterworth Q), so a band that leaves the chain before a later split goes through that split's all-pass too: then
//! every band carries the same phase and the bands sum to the input with only a phase shift (phase-coherent).
//! The interface draws the band shapes by the same formulas (`app/src/domain/multibandCurve.ts`). Pure DSP without
//! allocations, so it can move to the shared DSP crate (`docs/refactor-audit.md`, batch 6) as it is.

use crate::biquad::{BUTTERWORTH_Q, Biquad, Coefficients};

/// Splits between the bands.
pub const SPLITS: usize = 3;
pub const BANDS: usize = SPLITS + 1;

/// The filters of one split frequency.
#[derive(Clone, Copy)]
pub struct Split {
    low: Coefficients,
    high: Coefficients,
    all_pass: Coefficients,
}

impl Split {
    pub fn new(sample_rate: f32, freq: f32) -> Self {
        Self {
            low: Coefficients::low_pass(sample_rate, freq, BUTTERWORTH_Q),
            high: Coefficients::high_pass(sample_rate, freq, BUTTERWORTH_Q),
            all_pass: Coefficients::all_pass(sample_rate, freq, BUTTERWORTH_Q),
        }
    }
}

/// The crossover's filter state for one channel.
#[derive(Clone, Copy, Default)]
pub struct Crossover {
    low: [[Biquad; 2]; SPLITS],
    high: [[Biquad; 2]; SPLITS],
    /// `phase[band][split]`: the all-pass of a later split on a band that left the chain before it.
    phase: [[Biquad; SPLITS]; SPLITS],
}

impl Crossover {
    /// The bands of one sample, lowest first. The splits are taken in order up the chain: the low side of split `s` is
    /// band `s`, its high side goes on to the next split, and the high side of the last split is the top band.
    pub fn split(&mut self, splits: &[Split; SPLITS], input: f32) -> [f32; BANDS] {
        let mut bands = [0.0; BANDS];
        let mut rest = input;
        for (s, split) in splits.iter().enumerate() {
            let [low_a, low_b] = &mut self.low[s];
            let mut band = low_b.process(&split.low, low_a.process(&split.low, rest));
            let [high_a, high_b] = &mut self.high[s];
            rest = high_b.process(&split.high, high_a.process(&split.high, rest));
            for later in s + 1..SPLITS {
                band = self.phase[s][later].process(&splits[later].all_pass, band);
            }
            bands[s] = band;
        }
        bands[SPLITS] = rest;
        bands
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{SR, rms, sine, tone};

    fn splits() -> [Split; SPLITS] {
        [120.0, 1_000.0, 6_000.0].map(|hz| Split::new(SR as f32, hz))
    }

    /// The bands of a mono sine (as a "stereo" buffer whose channels hold the band and the sum).
    fn split_tone(hz: f32) -> ([Vec<f32>; BANDS], Vec<f32>) {
        let splits = splits();
        let mut crossover = Crossover::default();
        let input = sine(hz, 0.5, SR as usize);
        let mut bands: [Vec<f32>; BANDS] = Default::default();
        let mut sum = Vec::with_capacity(input.len());
        for frame in input.chunks_exact(2) {
            let parts = crossover.split(&splits, frame[0]);
            for (band, part) in bands.iter_mut().zip(parts) {
                band.extend([part, part]);
            }
            let total: f32 = parts.iter().sum();
            sum.extend([total, total]);
        }
        (bands, sum)
    }

    #[test]
    fn the_bands_sum_back_to_the_level_of_the_input() {
        for hz in [40.0, 120.0, 400.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0] {
            let (_, sum) = split_tone(hz);
            let settled = &sum[SR as usize..];
            assert!((tone(settled, hz) / 0.5 - 1.0).abs() < 0.01, "{hz} Hz: {}", tone(settled, hz));
        }
    }

    #[test]
    fn each_band_holds_its_own_range_and_the_splits_are_minus_6_db() {
        let (bands, _) = split_tone(400.0);
        let levels: Vec<f32> = bands.iter().map(|b| rms(&b[SR as usize..])).collect();
        assert!(levels[1] > levels[0] * 10.0 && levels[1] > levels[2] * 10.0 && levels[1] > levels[3] * 100.0, "{levels:?}");

        let (bands, _) = split_tone(1_000.0);
        let half = 0.5 * 0.5;
        assert!((tone(&bands[1][SR as usize..], 1_000.0) - half).abs() < 0.01, "low side at the split: −6 dB");
        assert!((tone(&bands[2][SR as usize..], 1_000.0) - half).abs() < 0.01, "high side at the split: −6 dB");
    }
}
