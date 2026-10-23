//! Designing the linear-phase filter: turn EQ settings into a kernel whose magnitude is the EQ's and whose phase is a
//! pure delay. The magnitude the EQ's biquads give (`eq::filters`, `Coefficients::magnitude`) is sampled on a grid of
//! `len` points with no phase of its own (zero phase), the inverse FFT gives a symmetric impulse, which is moved half
//! the kernel later (so it starts after 0) and windowed (Hann) to the kernel's length. The convolver needs the kernel's
//! spectrum on its grid of `2·len` points; once the delay's phase `e^(−iπk/2)` is taken out it is real, so the design is
//! `R(k)` for k = 0..=len (the rest mirrors it), and the convolver puts the delay back (`Convolver::offer`).
//! Allocates only in `new`; runs on the designer thread and, for a new sample rate, in `prepare` on the control side.

use crate::eq::{self, VALUES};
use crate::fft::{Complex, Fft};

/// The kernel length for a sample rate: about 85 ms of sound, so the low shelf and the low cut keep their shape down to
/// about 25 Hz (a Hann window blurs a response by about two bins of `rate / len`).
pub fn kernel_len(sample_rate: u32) -> usize {
    match sample_rate {
        0..=50_000 => 4_096,
        50_001..=100_000 => 8_192,
        _ => 16_384,
    }
}

/// The longest kernel (`kernel_len` at the highest rates): the exchange is sized for it.
pub const MAX_KERNEL: usize = 16_384;

/// `e^(iπk/2)`: 1, i, −1, −i — what takes the delay of half the kernel out of bin k of the `2·len` grid.
pub fn quarter_turns(k: usize) -> Complex {
    [Complex::new(1.0, 0.0), Complex::new(0.0, 1.0), Complex::new(-1.0, 0.0), Complex::new(0.0, -1.0)][k % 4]
}

pub struct Designer {
    len: usize,
    grid: Fft,
    kernel_fft: Fft,
    window: Vec<f32>,
    /// The zero-phase impulse on the `len` grid.
    impulse: Vec<Complex>,
    /// The windowed kernel padded to `2·len`, then its spectrum.
    kernel: Vec<Complex>,
    response: Vec<f32>,
}

impl Designer {
    pub fn new(len: usize) -> Self {
        let window = (0..len).map(|m| 0.5 - 0.5 * (std::f32::consts::TAU * m as f32 / len as f32).cos()).collect();
        Self {
            len,
            grid: Fft::new(len),
            kernel_fft: Fft::new(2 * len),
            window,
            impulse: vec![Complex::ZERO; len],
            kernel: vec![Complex::ZERO; 2 * len],
            response: vec![0.0; len + 1],
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// `R(k)`, k = 0..=len: the real response of the linear-phase kernel for the EQ's param `values` at `sample_rate`.
    pub fn design(&mut self, sample_rate: f32, values: &[f32; VALUES]) -> &[f32] {
        let len = self.len;
        let filters = eq::filters(sample_rate, values);
        for k in 0..=len / 2 {
            let freq = k as f32 * sample_rate / len as f32;
            let magnitude = filters.iter().map(|c| c.magnitude(sample_rate, freq)).product();
            self.impulse[k] = Complex::new(magnitude, 0.0);
            self.impulse[(len - k) % len] = Complex::new(magnitude, 0.0);
        }
        self.grid.inverse(&mut self.impulse);
        let half = len / 2;
        for (m, slot) in self.kernel.iter_mut().enumerate() {
            *slot = if m < len { Complex::new(self.impulse[(m + len - half) % len].re * self.window[m], 0.0) } else { Complex::ZERO };
        }
        self.kernel_fft.forward(&mut self.kernel);
        for (k, value) in self.response.iter_mut().enumerate() {
            *value = (self.kernel[k] * quarter_turns(k)).re;
        }
        &self.response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::PARAMS;

    fn flat() -> [f32; VALUES] {
        PARAMS.map(|p| p.default)
    }

    #[test]
    fn a_flat_eq_is_a_flat_response() {
        let mut designer = Designer::new(1_024);
        assert!(designer.design(48_000.0, &flat()).iter().all(|r| (r - 1.0).abs() < 1e-4));
    }

    #[test]
    fn the_response_follows_the_eq_magnitude() {
        let mut values = flat();
        values[3] = 12.0;
        let len = 4_096;
        let mut designer = Designer::new(len);
        let response = designer.design(48_000.0, &values).to_vec();
        let bin = |hz: f32| (hz * 2.0 * len as f32 / 48_000.0).round() as usize;
        let db = |r: f32| 20.0 * r.log10();
        assert!((db(response[bin(1_000.0)]) - 12.0).abs() < 0.2, "{}", db(response[bin(1_000.0)]));
        assert!(db(response[bin(100.0)]).abs() < 0.5, "{}", db(response[bin(100.0)]));
    }
}
