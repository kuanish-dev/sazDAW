//! Rounding audio to integer samples (16 or 24 bits), optionally with triangular noise (TPDF dither).
//!
//! Why noise: without it the rounding error follows the signal, and quiet tails (reverb decay) at 16 bits sound
//! "grainy", i.e. distorted. Triangular noise of ±1 least significant bit makes the error independent of the signal:
//! what remains is an even, quiet hiss. No noise shaping is done.

pub(crate) struct Quantizer {
    /// The largest sample: 1.0 maps to it (`i16::MAX` for 16 bits, as before without dither).
    max: f64,
    dither: Option<Tpdf>,
}

impl Quantizer {
    pub(crate) fn new(bits: u32, dither: bool) -> Self {
        Self { max: f64::from((1u32 << (bits - 1)) - 1), dither: dither.then(Tpdf::new) }
    }

    /// A sample in -1..1 (louder is clipped) as an integer. Computed in f64: f32 lacks precision for 24 bits with dither.
    pub(crate) fn quantize(&mut self, sample: f32) -> i32 {
        let noise = self.dither.as_mut().map_or(0.0, Tpdf::next);
        (f64::from(sample.clamp(-1.0, 1.0)) * self.max + noise).round().clamp(-self.max - 1.0, self.max) as i32
    }
}

/// Triangular noise in (-1, 1) LSB: the sum of two uniform values in [-1/2, 1/2). The generator is xorshift64 with a
/// fixed seed, so the same project always exports to the same file.
struct Tpdf {
    state: u64,
}

impl Tpdf {
    fn new() -> Self {
        Self { state: 0x9E37_79B9_7F4A_7C15 }
    }

    fn uniform(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    }

    fn next(&mut self) -> f64 {
        self.uniform() + self.uniform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_dither_rounds_like_before() {
        let mut q16 = Quantizer::new(16, false);
        assert_eq!([q16.quantize(1.0), q16.quantize(-1.0), q16.quantize(2.0), q16.quantize(0.5)], [32_767, -32_767, 32_767, 16_384]);
        let mut q24 = Quantizer::new(24, false);
        assert_eq!(q24.quantize(-2.0), -8_388_607);
    }

    /// The noise is triangular: in (-1, 1), mean ≈ 0, variance ≈ 1/6 (uniform ±1/2 has 1/12, the sum of two 1/6), with
    /// more values near zero than near the edges.
    #[test]
    fn noise_is_triangular_around_zero() {
        let mut tpdf = Tpdf::new();
        let values: Vec<f64> = (0..200_000).map(|_| tpdf.next()).collect();
        assert!(values.iter().all(|v| v.abs() < 1.0));
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        assert!(mean.abs() < 0.01, "{mean}");
        assert!((variance - 1.0 / 6.0).abs() < 0.005, "{variance}");
        let near_zero = values.iter().filter(|v| v.abs() < 0.25).count();
        let near_edges = values.iter().filter(|v| v.abs() > 0.75).count();
        assert!(near_zero > 4 * near_edges, "{near_zero} vs {near_edges}");
    }

    /// Dither changes only the lowest bit and does not shift the level: the mean of rounded values is the exact value
    /// (without dither a constant 0.3 LSB would always round to 0).
    #[test]
    fn dither_touches_only_the_lowest_bits_and_keeps_the_mean() {
        for bits in [16, 24] {
            let mut plain = Quantizer::new(bits, false);
            let mut dithered = Quantizer::new(bits, true);
            let scale = f64::from((1u32 << (bits - 1)) - 1);
            for value in [0.0f32, 0.3 / scale as f32, -0.25, 0.7] {
                let exact = f64::from(value) * scale;
                let samples: Vec<i32> = (0..100_000).map(|_| dithered.quantize(value)).collect();
                assert!(samples.iter().all(|&s| (f64::from(s) - exact).abs() < 1.5), "{bits} bits, {value}");
                assert!(samples.iter().all(|&s| (s - plain.quantize(value)).abs() <= 1));
                let mean = samples.iter().map(|&s| f64::from(s)).sum::<f64>() / samples.len() as f64;
                assert!((mean - exact).abs() < 0.01, "{bits} bits, {value}: mean {mean}, exact {exact}");
            }
        }
    }
}
