//! Input peak levels. The audio callback writes them (no allocations or locks), the interface polling thread reads them.

use std::sync::atomic::{AtomicU32, Ordering};

use daw_contracts::InputLevel;

/// How much the peak decays per block: the meter falls smoothly instead of jumping.
/// The writer applies the decay: there is a single writer, so reads and writes do not race.
const DECAY_PER_BLOCK: f32 = 0.97;

/// Levels of the first two channels; `f32` stored as bits in an atomic.
#[derive(Default)]
pub(crate) struct InputPeaks {
    left: AtomicU32,
    right: AtomicU32,
}

/// Block peaks of the first two channels; for mono the right equals the left.
pub(crate) fn block_peaks(interleaved: &[f32], channels: usize) -> (f32, f32) {
    if channels == 0 {
        return (0.0, 0.0);
    }
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for frame in interleaved.chunks_exact(channels) {
        let l = frame[0].abs();
        let r = if channels > 1 { frame[1].abs() } else { l };
        left = left.max(l);
        right = right.max(r);
    }
    (left.min(1.0), right.min(1.0))
}

impl InputPeaks {
    /// Called from the audio callback: arithmetic and atomics only.
    pub fn feed(&self, interleaved: &[f32], channels: usize) {
        let (left, right) = block_peaks(interleaved, channels);
        Self::decay_to(&self.left, left);
        Self::decay_to(&self.right, right);
    }

    fn decay_to(cell: &AtomicU32, block_peak: f32) {
        let held = f32::from_bits(cell.load(Ordering::Relaxed)) * DECAY_PER_BLOCK;
        cell.store(held.max(block_peak).to_bits(), Ordering::Relaxed);
    }

    pub fn read(&self) -> InputLevel {
        InputLevel { left: f32::from_bits(self.left.load(Ordering::Relaxed)), right: f32::from_bits(self.right.load(Ordering::Relaxed)) }
    }

    /// The input closed: the meter drops to zero at once.
    pub fn reset(&self) {
        self.left.store(0, Ordering::Relaxed);
        self.right.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_peaks_are_per_channel_absolute_and_clamped() {
        assert_eq!(block_peaks(&[0.1, -0.5, -0.4, 0.2, 0.3, 0.0], 2), (0.4, 0.5));
        assert_eq!(block_peaks(&[0.2, -0.7], 1), (0.7, 0.7), "mono goes to both sides");
        assert_eq!(block_peaks(&[0.1, 0.2, 0.9, 0.3, 0.4, 0.9], 3), (0.3, 0.4), "extra channels are ignored");
        assert_eq!(block_peaks(&[], 2), (0.0, 0.0));
        assert_eq!(block_peaks(&[1.0], 0), (0.0, 0.0));
        assert_eq!(block_peaks(&[3.0, -2.0], 2), (1.0, 1.0), "overload is clamped to one");
    }

    #[test]
    fn peak_falls_smoothly_and_resets_on_close() {
        let peaks = InputPeaks::default();
        peaks.feed(&[0.8, 0.8], 2);
        assert_eq!(peaks.read(), InputLevel { left: 0.8, right: 0.8 });
        peaks.feed(&[0.0, 0.0], 2);
        let after = peaks.read();
        assert!(after.left < 0.8 && after.left > 0.7, "the decay is smooth: {after:?}");
        peaks.feed(&[0.9, 0.1], 2);
        assert_eq!(peaks.read().left, 0.9, "a new peak above the held one is taken at once");
        peaks.reset();
        assert_eq!(peaks.read(), InputLevel::default());
    }
}
