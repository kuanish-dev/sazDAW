//! The synth's LFO: one slow wave shared by all voices (vibrato, filter wobble, tremolo); it runs free in hertz or
//! follows the song grid in note lengths.

use crate::osc::Noise;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LfoShape {
    Sine,
    Triangle,
    Saw,
    Square,
    /// A new random value every cycle, held until the next one (sample and hold).
    Random,
}

impl LfoShape {
    /// In the order of the `lfo_shape` parameter options.
    pub(crate) const ALL: [LfoShape; 5] = [LfoShape::Sine, LfoShape::Triangle, LfoShape::Saw, LfoShape::Square, LfoShape::Random];

    pub(crate) fn from_index(value: f32) -> Self {
        Self::ALL[(value.round().max(0.0) as usize).min(Self::ALL.len() - 1)]
    }
}

/// Cycle lengths of the `lfo_sync` options after "Off", in beats (quarter notes): 1/1, 1/2, 1/4, 1/8, 1/16 and the
/// triplets of 1/4, 1/8, 1/16.
pub(crate) const SYNC_BEATS: [f64; 8] = [4.0, 2.0, 1.0, 0.5, 0.25, 2.0 / 3.0, 1.0 / 3.0, 1.0 / 6.0];

/// The cycle length in beats of `lfo_sync` option `index`; `None` for "Off" (the LFO runs in hertz).
pub(crate) fn sync_beats(index: f32) -> Option<f64> {
    let index = index.round().max(0.0) as usize;
    index.checked_sub(1).map(|i| SYNC_BEATS[i.min(SYNC_BEATS.len() - 1)])
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Lfo {
    /// Position in the cycle, `0..1`.
    phase: f64,
    /// The value of the `Random` shape for the current cycle.
    held: f32,
    noise: Noise,
}

impl Lfo {
    pub(crate) const fn new() -> Self {
        Self { phase: 0.0, held: 0.0, noise: Noise::new() }
    }

    /// Puts the phase where the song grid has it at `beat`: a synced LFO stays on the beat after a seek or a loop.
    pub(crate) fn align(&mut self, beat: f64, cycle_beats: f64) {
        self.phase = (beat / cycle_beats).rem_euclid(1.0);
    }

    /// The value at the current phase (`-1..=1`), then moves on by `cycles` (a fraction of a cycle).
    pub(crate) fn advance(&mut self, shape: LfoShape, cycles: f64) -> f32 {
        let t = self.phase as f32;
        let value = match shape {
            LfoShape::Sine => (std::f32::consts::TAU * t).sin(),
            LfoShape::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            LfoShape::Saw => 2.0 * t - 1.0,
            LfoShape::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoShape::Random => self.held,
        };
        self.phase += cycles;
        if self.phase >= 1.0 {
            self.phase = self.phase.fract();
            self.held = self.noise.sample();
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_stays_within_one_and_repeats_each_cycle() {
        for shape in LfoShape::ALL.into_iter().filter(|s| *s != LfoShape::Random) {
            let mut lfo = Lfo::new();
            let first: Vec<f32> = (0..64).map(|_| lfo.advance(shape, 1.0 / 64.0)).collect();
            let second: Vec<f32> = (0..64).map(|_| lfo.advance(shape, 1.0 / 64.0)).collect();
            assert!(first.iter().all(|v| v.abs() <= 1.0), "{shape:?}");
            assert!(first.iter().zip(&second).all(|(a, b)| (a - b).abs() < 1e-3), "{shape:?}");
        }
    }

    #[test]
    fn random_holds_a_value_for_a_whole_cycle() {
        let mut lfo = Lfo::new();
        lfo.advance(LfoShape::Random, 1.0);
        let cycle: Vec<f32> = (0..8).map(|_| lfo.advance(LfoShape::Random, 0.125)).collect();
        assert!(cycle.iter().all(|v| *v == cycle[0]), "{cycle:?}");
        assert_ne!(lfo.advance(LfoShape::Random, 0.125), cycle[0], "the next cycle has a new value");
    }

    #[test]
    fn sync_options_are_note_lengths_and_off_is_none() {
        assert_eq!(sync_beats(0.0), None);
        assert_eq!(sync_beats(1.0), Some(4.0));
        assert_eq!(sync_beats(4.0), Some(0.5));
        assert_eq!(sync_beats(99.0), Some(1.0 / 6.0));
    }

    #[test]
    fn align_puts_the_phase_on_the_grid() {
        let mut lfo = Lfo::new();
        lfo.align(9.5, 1.0);
        assert!((lfo.advance(LfoShape::Saw, 0.0) - 0.0).abs() < 1e-6, "half a beat into a one-beat cycle is the middle of the saw");
    }
}
