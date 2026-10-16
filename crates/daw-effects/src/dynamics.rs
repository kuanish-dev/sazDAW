//! Shared dynamics blocks of the compressor, the multiband compressor, the de-esser, the dynamic EQ and the transient
//! shaper: the level of a stereo frame, the soft-knee gain computer (how many dB to take off a level) and an
//! attack/release follower. The interface draws the gain computer's curve by the same formula
//! (`app/src/domain/compressorCurve.ts: targetReduction`): a change here must be mirrored there. Pure math without
//! allocations, so it can move to the shared DSP crate (`docs/refactor-audit.md`, batch 6) as it is.

use crate::time_coefficient;

/// Level of a stereo frame for the detectors: the louder channel in dBFS, silence floored at −120 dB.
pub fn peak_db(left: f32, right: f32) -> f32 {
    20.0 * left.abs().max(right.abs()).max(1e-6).log10()
}

/// The static curve of a compressor: threshold and knee in dB, the ratio above the knee.
#[derive(Clone, Copy)]
pub struct GainComputer {
    pub threshold: f32,
    pub ratio: f32,
    pub knee: f32,
}

impl GainComputer {
    /// Gain reduction (dB, 0 or more) needed for the level `level_db`.
    pub fn reduction(self, level_db: f32) -> f32 {
        let over = level_db - self.threshold;
        let slope = 1.0 - 1.0 / self.ratio;
        if 2.0 * over < -self.knee {
            0.0
        } else if 2.0 * over.abs() <= self.knee {
            slope * (over + self.knee / 2.0).powi(2) / (2.0 * self.knee.max(f32::EPSILON))
        } else {
            slope * over
        }
    }
}

/// A value that rises toward a larger target with the attack time and falls toward a smaller one with the release time
/// (one-pole each way): the ballistics of a gain reduction in dB, or an envelope of a level.
#[derive(Clone, Copy, Default)]
pub struct Follower {
    attack: f32,
    release: f32,
    value: f32,
}

impl Follower {
    /// The attack and release times; 0 ms is instant (the value jumps to its target that way).
    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sample_rate: f32) {
        let coefficient = |ms: f32| if ms > 0.0 { time_coefficient(ms, sample_rate) } else { 0.0 };
        self.attack = coefficient(attack_ms);
        self.release = coefficient(release_ms);
    }

    /// One frame step toward `target`; returns the new value.
    pub fn follow(&mut self, target: f32) -> f32 {
        let coefficient = if target > self.value { self.attack } else { self.release };
        self.value = target + coefficient * (self.value - target);
        self.value
    }

    pub fn value(self) -> f32 {
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_starts_at_the_threshold_and_follows_the_ratio() {
        let hard = GainComputer { threshold: -20.0, ratio: 4.0, knee: 0.0 };
        assert_eq!(hard.reduction(-30.0), 0.0);
        assert!((hard.reduction(-8.0) - 9.0).abs() < 1e-4, "12 dB over at 4:1 → 9 dB off");
        let soft = GainComputer { knee: 6.0, ..hard };
        assert_eq!(soft.reduction(-24.0), 0.0, "below the knee");
        assert!(soft.reduction(-20.0) > 0.0 && soft.reduction(-20.0) < 1.0, "inside the knee: a little");
        assert!((soft.reduction(-8.0) - 9.0).abs() < 1e-4, "above the knee: the ratio");
    }

    #[test]
    fn the_follower_rises_with_the_attack_and_falls_with_the_release() {
        let mut follower = Follower::default();
        follower.set_times(1.0, 100.0, 48_000.0);
        for _ in 0..480 {
            follower.follow(10.0);
        }
        assert!(follower.value() > 9.9, "10 attack times: {}", follower.value());
        for _ in 0..480 {
            follower.follow(0.0);
        }
        assert!(follower.value() > 8.5, "a tenth of the release time: {}", follower.value());
        follower.reset();
        assert_eq!(follower.value(), 0.0);

        follower.set_times(0.0, 100.0, 48_000.0);
        assert_eq!(follower.follow(0.7), 0.7, "0 ms is instant");
    }
}
