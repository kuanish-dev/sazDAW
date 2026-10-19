//! A radix-2 complex FFT for the FFT-based effects (the linear-phase EQ): iterative Cooley–Tukey on a power-of-two
//! size, with the twiddles and the bit-reversed order computed once in `new`, so a transform never allocates and can run
//! on the audio thread. The forward transform is unscaled, the inverse one divides by the size. Pure math, so it can
//! move to the shared DSP crate (`docs/refactor-audit.md`, batch 6) as it is.

use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    pub fn scale(self, k: f32) -> Self {
        Self::new(self.re * k, self.im * k)
    }
}

impl Add for Complex {
    type Output = Complex;
    fn add(self, other: Complex) -> Complex {
        Complex::new(self.re + other.re, self.im + other.im)
    }
}

impl Sub for Complex {
    type Output = Complex;
    fn sub(self, other: Complex) -> Complex {
        Complex::new(self.re - other.re, self.im - other.im)
    }
}

impl Mul for Complex {
    type Output = Complex;
    fn mul(self, other: Complex) -> Complex {
        Complex::new(self.re * other.re - self.im * other.im, self.re * other.im + self.im * other.re)
    }
}

pub struct Fft {
    /// `e^(−2πik/size)` for k below half the size.
    twiddles: Vec<Complex>,
    /// Where each index goes in the bit-reversed order.
    reversed: Vec<usize>,
}

impl Fft {
    /// A transform of `size` points; `size` must be a power of two (2 or more).
    pub fn new(size: usize) -> Self {
        debug_assert!(size.is_power_of_two() && size >= 2);
        let bits = size.trailing_zeros();
        let twiddles = (0..size / 2)
            .map(|k| {
                // In f64: the twiddles of a long transform keep their accuracy.
                let angle = -std::f64::consts::TAU * k as f64 / size as f64;
                Complex::new(angle.cos() as f32, angle.sin() as f32)
            })
            .collect();
        let reversed = (0..size).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect();
        Self { twiddles, reversed }
    }

    pub fn size(&self) -> usize {
        self.reversed.len()
    }

    /// `data` (exactly `size` points) → its spectrum, in place.
    pub fn forward(&self, data: &mut [Complex]) {
        self.transform(data, false);
    }

    /// A spectrum → the signal, in place, scaled so `inverse(forward(x)) = x`.
    pub fn inverse(&self, data: &mut [Complex]) {
        self.transform(data, true);
        let scale = 1.0 / self.size() as f32;
        for value in data {
            *value = value.scale(scale);
        }
    }

    fn transform(&self, data: &mut [Complex], inverse: bool) {
        let size = self.size();
        for (i, &j) in self.reversed.iter().enumerate() {
            if j > i {
                data.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= size {
            let half = len / 2;
            let step = size / len;
            for start in (0..size).step_by(len) {
                for k in 0..half {
                    let twiddle = self.twiddles[k * step];
                    let twiddle = if inverse { twiddle.conj() } else { twiddle };
                    let a = data[start + k];
                    let b = data[start + k + half] * twiddle;
                    data[start + k] = a + b;
                    data[start + k + half] = a - b;
                }
            }
            len *= 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lands_in_its_bin_and_the_inverse_gives_it_back() {
        let size = 64;
        let fft = Fft::new(size);
        let signal: Vec<Complex> =
            (0..size).map(|i| Complex::new((std::f32::consts::TAU * 5.0 * i as f32 / size as f32).cos(), 0.0)).collect();
        let mut data = signal.clone();
        fft.forward(&mut data);
        for (k, bin) in data.iter().enumerate() {
            let expected = if k == 5 || k == size - 5 { size as f32 / 2.0 } else { 0.0 };
            assert!((bin.re - expected).abs() < 1e-3 && bin.im.abs() < 1e-3, "bin {k}: {bin:?}");
        }
        fft.inverse(&mut data);
        for (a, b) in data.iter().zip(&signal) {
            assert!((a.re - b.re).abs() < 1e-5 && a.im.abs() < 1e-5);
        }
    }

    #[test]
    fn multiplying_spectra_convolves_circularly() {
        let size = 8;
        let fft = Fft::new(size);
        let mut x: Vec<Complex> = [1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0].map(|v| Complex::new(v, 0.0)).to_vec();
        let mut h: Vec<Complex> = [0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0].map(|v| Complex::new(v, 0.0)).to_vec();
        fft.forward(&mut x);
        fft.forward(&mut h);
        let mut y: Vec<Complex> = x.iter().zip(&h).map(|(a, b)| *a * *b).collect();
        fft.inverse(&mut y);
        let expected = [0.0, 1.0, 2.5, 4.0, 1.5, 0.0, 0.0, 0.0];
        for (got, want) in y.iter().zip(expected) {
            assert!((got.re - want).abs() < 1e-5, "{y:?}");
        }
    }
}
