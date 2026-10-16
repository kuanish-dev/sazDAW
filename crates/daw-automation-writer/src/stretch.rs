//! One stretch of writing a target into one clip pattern: from the touch (or a loop jump) until release, idle timeout,
//! stop or the end of the clip. Bookkeeping only — which points the pattern gets (ticks are pattern ticks); the writer
//! makes the project edits.
//!
//! The span `start..=end` replaces the pattern points that were there before the stretch (`original`). Joining points
//! keep the rest of the pattern exactly as it was: one a tick before the span (the old value there) and, unless the
//! clip grows with the writing (nothing after the span yet), one a tick after it (the old value again), so the old
//! curve does not slide into the written values.
//! A dense stream is thinned: a move closer than [`MIN_GAP_TICKS`] to the last kept point only updates that point's
//! value (the last value is never lost).

use daw_contracts::{AutoPoint, value_at};

/// Moves closer than this (≈ 15 ms at 120 BPM) to the last kept point are merged into it.
pub(crate) const MIN_GAP_TICKS: u32 = 30;

pub(crate) struct Stretch {
    /// Lane points before the stretch.
    original: Vec<AutoPoint>,
    start: u32,
    /// The furthest play position reached while writing.
    end: u32,
    /// Written points by ascending tick (straight segments).
    recorded: Vec<AutoPoint>,
    /// Keep the old curve after the span (an existing clip); `false` for a clip growing with the writing.
    join_after: bool,
}

impl Stretch {
    pub(crate) fn begin(original: Vec<AutoPoint>, tick: u32, value: f32, join_after: bool) -> Self {
        Self { original, start: tick, end: tick, recorded: vec![AutoPoint::new(tick, value)], join_after }
    }

    pub(crate) fn end(&self) -> u32 {
        self.end
    }

    /// The last written value.
    pub(crate) fn value(&self) -> f32 {
        self.recorded.last().map_or(0.0, |p| p.value)
    }

    /// A new value at `tick` (not before the stretch end: a jump back is a new stretch, the writer decides).
    pub(crate) fn write(&mut self, tick: u32, value: f32) {
        let tick = tick.max(self.end);
        self.end = tick;
        match self.recorded.last_mut() {
            Some(last) if tick - last.tick < MIN_GAP_TICKS => last.value = value,
            _ => self.recorded.push(AutoPoint::new(tick, value)),
        }
    }

    /// The play position moved on; the last value holds up to `tick`.
    pub(crate) fn extend(&mut self, tick: u32) -> bool {
        if tick <= self.end {
            return false;
        }
        self.end = tick;
        true
    }

    /// The whole pattern: original points outside the span, the joins and the written points.
    pub(crate) fn lane(&self) -> Vec<AutoPoint> {
        let (lo, hi) = (self.start, self.end);
        let mut points: Vec<AutoPoint> = self.original.iter().copied().filter(|p| p.tick < lo || p.tick > hi).collect();
        let free = |points: &[AutoPoint], tick: u32| !points.iter().any(|p| p.tick == tick);
        if lo > 0
            && free(&points, lo - 1)
            && let Some(before) = value_at(&self.original, f64::from(lo - 1))
        {
            points.push(AutoPoint::new(lo - 1, before));
        }
        if self.join_after
            && let Some(after) = value_at(&self.original, f64::from(hi) + 1.0)
            && free(&points, hi + 1)
        {
            points.push(AutoPoint::new(hi + 1, after));
        }
        points.extend(self.recorded.iter().copied());
        if self.recorded.last().is_some_and(|p| p.tick < hi) {
            points.push(AutoPoint::new(hi, self.value()));
        }
        points.sort_by_key(|p| p.tick);
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticks_values(points: &[AutoPoint]) -> Vec<(u32, f32)> {
        points.iter().map(|p| (p.tick, p.value)).collect()
    }

    #[test]
    fn a_stretch_on_an_empty_lane_is_just_the_written_points() {
        let mut stretch = Stretch::begin(Vec::new(), 100, 0.5, true);
        stretch.write(200, 0.7);
        stretch.write(210, 0.8);
        stretch.extend(400);
        assert_eq!(ticks_values(&stretch.lane()), [(100, 0.5), (200, 0.8), (400, 0.8)], "a close move updates the last point");
    }

    #[test]
    fn the_span_replaces_old_points_and_joins_keep_the_rest() {
        let original = vec![AutoPoint::new(0, 0.0), AutoPoint::new(300, 1.0), AutoPoint::new(1_000, 0.0)];
        let mut stretch = Stretch::begin(original, 200, 0.5, true);
        stretch.extend(500);
        let lane = ticks_values(&stretch.lane());
        assert_eq!(lane[0], (0, 0.0));
        assert_eq!(lane[1].0, 199);
        assert!((lane[1].1 - 199.0 / 300.0).abs() < 1e-6, "the old value just before the span");
        assert_eq!(&lane[2..4], [(200, 0.5), (500, 0.5)], "the 300 point is overwritten");
        assert_eq!(lane[4].0, 501);
        assert!((lane[4].1 - (1.0 - 201.0 / 700.0)).abs() < 1e-5, "the old value just after the span");
        assert_eq!(lane[5], (1_000, 0.0));
    }

    #[test]
    fn a_growing_clip_has_no_join_after_the_span() {
        let mut stretch = Stretch::begin(vec![AutoPoint::new(0, 0.3)], 200, 0.5, false);
        stretch.extend(500);
        assert_eq!(ticks_values(&stretch.lane()), [(0, 0.3), (199, 0.3), (200, 0.5), (500, 0.5)]);
    }
}
