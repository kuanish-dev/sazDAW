//! Waveshaping at twice the sample rate, for the saturating effects (saturator, tape): each frame becomes two steps —
//! halfway to it by linear interpolation, then the frame itself — both go through the curve, and a 4th-order Butterworth
//! low-pass just below the original Nyquist takes what folded above it away before every second step is kept. This
//! noticeably reduces aliasing from the harmonics a curve makes above Nyquist, though it does not remove it fully,
//! which is enough for a built-in effect. Pure DSP without allocations, so it can move to the shared DSP crate
//! (`docs/refactor-audit.md`, batch 6) as it is.

use crate::biquad::{Biquad, Coefficients};

/// Q of the two sections of the 4th-order Butterworth filter (flat passband).
const DECIMATION_Q: [f32; 2] = [0.541_196_1, 1.306_563];
/// Decimation filter cutoff as a fraction of the original sample rate (just below half of it).
const DECIMATION_CUTOFF: f32 = 0.45;

/// The decimation filter for a sample rate (shared by every channel).
#[derive(Clone, Copy)]
pub struct Decimation([Coefficients; 2]);

impl Decimation {
    pub fn new(sample_rate: f32) -> Self {
        let cutoff = sample_rate * DECIMATION_CUTOFF;
        Self(DECIMATION_Q.map(|q| Coefficients::low_pass(sample_rate * 2.0, cutoff, q)))
    }
}

/// One channel's state.
#[derive(Clone, Copy, Default)]
pub struct Oversampled {
    previous: f32,
    filters: [Biquad; 2],
}

impl Oversampled {
    /// One frame of `input` through `curve` at twice the rate.
    pub fn process(&mut self, decimation: &Decimation, input: f32, curve: impl Fn(f32) -> f32) -> f32 {
        let halfway = f32::midpoint(self.previous, input);
        self.previous = input;
        let mut output = 0.0;
        for x in [halfway, input] {
            let first = self.filters[0].process(&decimation.0[0], curve(x));
            output = self.filters[1].process(&decimation.0[1], first);
        }
        output
    }
}
