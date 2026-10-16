//! A smoothed parameter value: the engine changes parameters once per block (automation included), and without
//! smoothing a sharp step is heard as a click or zipper noise. The value moves to its target through a one-pole filter.

use crate::time_coefficient;

/// Parameter smoothing time constant: shorter makes steps audible, longer makes the knob lag behind the hand.
pub const SMOOTH_MS: f32 = 10.0;

/// Smoothing coefficient of `SMOOTH_MS` for a sample rate.
pub fn smooth_coefficient(sample_rate: f32) -> f32 {
    time_coefficient(SMOOTH_MS, sample_rate)
}

#[derive(Clone, Copy)]
pub struct Smoothed {
    current: f32,
    target: f32,
}

impl Smoothed {
    pub fn new(value: f32) -> Self {
        Self { current: value, target: value }
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Straight to the target: after an effect reset there is nothing to catch up, and a new effect must not glide into its values.
    pub fn snap(&mut self) {
        self.current = self.target;
    }

    pub fn is_settled(self) -> bool {
        self.current == self.target
    }

    /// The next value (one frame step). Very close to the target it snaps to it, so `is_settled` is eventually reached.
    pub fn next(&mut self, coefficient: f32) -> f32 {
        self.current = self.target + coefficient * (self.current - self.target);
        if (self.current - self.target).abs() < 1e-4 {
            self.current = self.target;
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_towards_the_target_and_settles() {
        let mut value = Smoothed::new(0.0);
        value.set(1.0);
        let coefficient = smooth_coefficient(48_000.0);
        let first = value.next(coefficient);
        assert!(first > 0.0 && first < 0.01, "the first step is small: {first}");
        for _ in 0..48_000 {
            value.next(coefficient);
        }
        assert!(value.is_settled());
        value.set(5.0);
        value.snap();
        assert!(value.is_settled() && value.next(coefficient) == 5.0);
    }
}
