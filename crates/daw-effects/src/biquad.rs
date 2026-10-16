//! Biquad filter (RBJ Audio EQ Cookbook formulas). The EQ graph in the interface draws the response from the same
//! formulas (`app/src/domain/eqCurve.ts`): a change here must be mirrored there.

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
