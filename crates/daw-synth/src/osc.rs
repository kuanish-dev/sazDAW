//! The synth's oscillator shapes and its white noise source.

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
    /// White noise: has no pitch, so the phase is ignored (risers, hats, breath on a pad).
    Noise,
}

impl Waveform {
    /// In the order of the `waveform` / `osc2_waveform` parameter options.
    pub(crate) const ALL: [Waveform; 5] = [Waveform::Sine, Waveform::Triangle, Waveform::Saw, Waveform::Square, Waveform::Noise];

    /// The option by its index (the parameter value); out-of-range values take the nearest option.
    pub(crate) fn from_index(value: f32) -> Self {
        Self::ALL[(value.round().max(0.0) as usize).min(Self::ALL.len() - 1)]
    }
}

/// Waveform at phase `t` (`0..1` of a cycle); `noise` is this frame's noise sample. The result is in `[-1, 1]`.
pub(crate) fn wave(waveform: Waveform, t: f32, noise: f32) -> f32 {
    match waveform {
        Waveform::Sine => (TAU * t).sin(),
        Waveform::Triangle => 4.0 * (t - 0.5).abs() - 1.0,
        Waveform::Saw => 2.0 * t - 1.0,
        Waveform::Square => {
            if t < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Waveform::Noise => noise,
    }
}

/// White noise from a xorshift generator: no allocations on the audio thread, the same sequence after every reset
/// (so renders of the same project are identical).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Noise(u32);

impl Noise {
    const SEED: u32 = 0x9E37_79B9;

    pub(crate) const fn new() -> Self {
        Self(Self::SEED)
    }

    /// The next sample, uniform in `[-1, 1)`.
    pub(crate) fn sample(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 23) as f32 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_waveform_stays_within_one() {
        let mut noise = Noise::new();
        for waveform in Waveform::ALL {
            assert!((0..64).all(|i| wave(waveform, i as f32 / 64.0, noise.sample()).abs() <= 1.0), "{waveform:?}");
        }
    }

    #[test]
    fn waveform_index_picks_the_nearest_option() {
        assert_eq!(Waveform::from_index(0.0), Waveform::Sine);
        assert_eq!(Waveform::from_index(2.4), Waveform::Saw);
        assert_eq!(Waveform::from_index(4.0), Waveform::Noise);
        assert_eq!(Waveform::from_index(9.0), Waveform::Noise);
        assert_eq!(Waveform::from_index(-3.0), Waveform::Sine);
    }

    #[test]
    fn noise_is_centred_and_does_not_repeat_soon() {
        let mut noise = Noise::new();
        let samples: Vec<f32> = (0..48_000).map(|_| noise.sample()).collect();
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < 0.02, "{mean}");
        assert!(samples.iter().all(|s| (-1.0..1.0).contains(s)));
        assert_ne!(samples[..100], samples[100..200]);
        assert_eq!(Noise::new().sample(), samples[0], "the same sequence after a reset");
    }
}
