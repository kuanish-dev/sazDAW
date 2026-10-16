//! Biquad filter (RBJ Audio EQ Cookbook formulas). The effect graphs in the interface draw their responses from the
//! same formulas (`app/src/domain/biquad.ts`): a change here must be mirrored there.

use std::f32::consts::TAU;

/// Butterworth Q: the flattest passband with no bump at the cutoff.
pub const BUTTERWORTH_Q: f32 = std::f32::consts::FRAC_1_SQRT_2;

#[derive(Clone, Copy, Default)]
pub struct Coefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Coefficients {
    pub const PASS: Coefficients = Coefficients { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn normalized(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    pub fn low_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        Self::shelf(sample_rate, freq, gain_db, 1.0)
    }

    pub fn high_shelf(sample_rate: f32, freq: f32, gain_db: f32) -> Self {
        Self::shelf(sample_rate, freq, gain_db, -1.0)
    }

    /// Shelf: the RBJ low and high shelf formulas differ only in the sign of `cos` and of `b1`/`a1`
    /// (`side` = 1 low, −1 high).
    fn shelf(sample_rate: f32, freq: f32, gain_db: f32, side: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w = TAU * freq / sample_rate;
        let (sin, cos) = w.sin_cos();
        let cos = side * cos;
        let beta = 2.0 * a.sqrt() * (sin / 2.0 * 2f32.sqrt());
        Self::normalized(
            a * ((a + 1.0) - (a - 1.0) * cos + beta),
            side * 2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - beta),
            (a + 1.0) + (a - 1.0) * cos + beta,
            side * -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - beta,
        )
    }

    pub fn peaking(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w = TAU * freq / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a)
    }

    pub fn high_pass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let w = TAU * freq.min(sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(f32::midpoint(1.0, cos), -(1.0 + cos), f32::midpoint(1.0, cos), 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    pub fn low_pass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let w = TAU * freq.min(sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    /// Band-pass with 0 dB at its centre. With a notch of the same frequency and Q it sums to the input exactly
    /// (the numerators add up to the shared denominator), so `input − (1 − g) · band` turns only this band by `g`.
    pub fn band_pass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let w = TAU * freq.min(sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    /// All-pass: a flat magnitude with the phase of a 2nd-order section. At Butterworth Q it is exactly the sum of the
    /// two sides of a Linkwitz–Riley split at the same frequency (`crossover.rs`), so the frequency is clamped the same
    /// way as the low- and high-pass.
    pub fn all_pass(sample_rate: f32, freq: f32, q: f32) -> Self {
        let w = TAU * freq.min(sample_rate * 0.45) / sample_rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(1.0 - alpha, -2.0 * cos, 1.0 + alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
    }

    /// The magnitude of the response at `freq`, as a gain (1 = 0 dB): what the filter does to a steady tone there.
    pub fn magnitude(&self, sample_rate: f32, freq: f32) -> f32 {
        let w = TAU * freq / sample_rate;
        let (sin1, cos1) = w.sin_cos();
        let (sin2, cos2) = (2.0 * w).sin_cos();
        let num = (self.b0 + self.b1 * cos1 + self.b2 * cos2).powi(2) + (self.b1 * sin1 + self.b2 * sin2).powi(2);
        let den = (1.0 + self.a1 * cos1 + self.a2 * cos2).powi(2) + (self.a1 * sin1 + self.a2 * sin2).powi(2);
        (num / den.max(1e-20)).sqrt()
    }
}

/// State of one channel (transposed direct form II).
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn process(&mut self, c: &Coefficients, input: f32) -> f32 {
        let output = c.b0 * input + self.z1;
        self.z1 = c.b1 * input - c.a1 * output + self.z2;
        self.z2 = c.b2 * input - c.a2 * output;
        output
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// First-order DC blocker: a high-pass a few hertz up, for curves that offset the signal (asymmetric saturation).
#[derive(Clone, Copy, Default)]
pub struct DcBlocker {
    input: f32,
    output: f32,
}

impl DcBlocker {
    /// The pole for a cutoff of `hz`.
    pub fn pole(sample_rate: f32, hz: f32) -> f32 {
        (-TAU * hz / sample_rate).exp()
    }

    pub fn process(&mut self, pole: f32, input: f32) -> f32 {
        let output = input - self.input + pole * self.output;
        self.input = input;
        self.output = output;
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{SR, sine, tone};

    /// The gain of the filter for a steady tone of `hz` (half a second to settle, half to measure).
    fn gain_at(c: &Coefficients, hz: f32) -> f32 {
        let mut state = Biquad::default();
        let output: Vec<f32> = sine(hz, 0.5, SR as usize)
            .into_iter()
            .step_by(2)
            .flat_map(|v| {
                let y = state.process(c, v);
                [y, y]
            })
            .collect();
        tone(&output[SR as usize..], hz) / 0.5
    }

    #[test]
    fn the_all_pass_keeps_the_level_of_every_tone() {
        let all = Coefficients::all_pass(SR as f32, 1_000.0, BUTTERWORTH_Q);
        for hz in [30.0, 300.0, 1_000.0, 3_000.0, 15_000.0] {
            assert!((gain_at(&all, hz) - 1.0).abs() < 0.01, "{hz} Hz");
        }
    }

    #[test]
    fn magnitude_is_the_gain_a_steady_tone_gets() {
        let peak = Coefficients::peaking(SR as f32, 1_000.0, 12.0, 1.0);
        for hz in [200.0, 1_000.0, 3_000.0] {
            assert!((peak.magnitude(SR as f32, hz) - gain_at(&peak, hz)).abs() < 0.01, "{hz} Hz");
        }
    }

    #[test]
    fn the_band_pass_is_0_db_at_its_centre_and_falls_away_from_it() {
        let band = Coefficients::band_pass(SR as f32, 6_000.0, 1.0);
        assert!((gain_at(&band, 6_000.0) - 1.0).abs() < 0.01);
        assert!(gain_at(&band, 600.0) < 0.2);
    }
}
