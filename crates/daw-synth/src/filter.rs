//! The voice filter: a resonant 12 dB/octave lowpass. It is a state-variable filter in the topology-preserving form,
//! so it stays stable and does not click while the cutoff moves every frame (filter envelope, LFO, mod wheel).

use std::f32::consts::PI;

/// How far the resonance knob at its maximum lowers the damping (2 is a smooth corner, 0 self-oscillation): 1.9 leaves
/// a strong peak (Q 10) that never runs away.
const MAX_RESONANCE: f32 = 1.9;

/// Coefficients for one cutoff and resonance; computed once per frame and shared by a voice's two channels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Coefficients {
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Coefficients {
    /// `resonance` is `0..=1`: 0 is a smooth corner (Q 0.5, no peak), 1 a sharp peak at the cutoff.
    pub(crate) fn lowpass(cutoff_hz: f32, resonance: f32, sample_rate: f32) -> Self {
        let g = (PI * cutoff_hz / sample_rate).tan();
        let k = 2.0 - MAX_RESONANCE * resonance.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self { a1, a2, a3: g * a2 }
    }
}

/// The filter's memory (one channel).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub(crate) const REST: Svf = Svf { ic1: 0.0, ic2: 0.0 };

    pub(crate) fn lowpass(&mut self, c: &Coefficients, x: f32) -> f32 {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Peak of a sine at `hz` after the filter settles.
    fn gain(hz: f32, cutoff: f32, resonance: f32) -> f32 {
        let c = Coefficients::lowpass(cutoff, resonance, SR);
        let mut svf = Svf::REST;
        let out: Vec<f32> = (0..9_600).map(|i| svf.lowpass(&c, (2.0 * PI * hz * i as f32 / SR).sin())).collect();
        out[4_800..].iter().fold(0.0, |a, b| a.max(b.abs()))
    }

    #[test]
    fn passes_lows_and_cuts_highs_by_twelve_db_per_octave() {
        assert!((gain(100.0, 1_000.0, 0.0) - 1.0).abs() < 0.02);
        let (one, two) = (gain(4_000.0, 1_000.0, 0.0), gain(8_000.0, 1_000.0, 0.0));
        assert!(one < 0.08 && two < one / 3.0, "an octave higher is about four times quieter: {one} → {two}");
    }

    #[test]
    fn resonance_lifts_the_cutoff_and_stays_stable() {
        assert!(gain(1_000.0, 1_000.0, 0.0) < 0.55, "a smooth corner is about -6 dB at the cutoff");
        assert!(gain(1_000.0, 1_000.0, 1.0) > 5.0, "full resonance is a sharp peak");
        assert!(gain(100.0, 1_000.0, 1.0) < 1.1, "far below the cutoff nothing changes");
    }
}
