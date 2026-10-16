//! Take controllers: pitch bend, CC knobs and pressure played live become points of the take clip controller lane.
//! Bookkeeping only; the recorder makes the project edits.
//!
//! A dense stream is thinned: a point is kept only if the value changed by more than 1/127 (a 7-bit CC step) or more
//! than 10 ms passed since the last kept one; a small frequent change is deferred and kept when the stream goes quiet
//! (end of take), so the last knob value is never lost. The span recorded on a pass replaces the lane points that were
//! inside it before the take; the next pass replaces what earlier passes recorded (as with notes).

use std::collections::HashMap;

use daw_contracts::{ClipId, ControllerKind, ControllerPoint, TrackId};

use crate::take::Take;

/// A change smaller than a 7-bit CC step: jitter or a 14-bit stream.
pub(crate) const MIN_STEP: f32 = 1.0 / 127.0;
/// Small changes more frequent than this are not kept.
pub(crate) const MIN_GAP_SECONDS: f64 = 0.010;

/// Recording of one lane (track + controller kind) in this take.
struct Recording {
    clip: ClipId,
    /// Lane points in the clip before the take.
    original: Vec<ControllerPoint>,
    pass: u64,
    /// Recorded on the current pass, by ascending tick.
    recorded: Vec<ControllerPoint>,
    /// The last kept point: (song seconds, value).
    last_kept: Option<(f64, f32)>,
    /// A deferred small change.
    pending: Option<ControllerPoint>,
    /// There is something to write to the project.
    dirty: bool,
}

impl Recording {
    fn keep(&mut self, point: ControllerPoint, seconds: f64) {
        self.pending = None;
        self.last_kept = Some((seconds, point.value));
        match self.recorded.last_mut() {
            // The engine stamps events with the segment start tick: of several events on one tick the last one stays.
            Some(last) if last.tick == point.tick => last.value = point.value,
            _ => self.recorded.push(point),
        }
        self.dirty = true;
    }

    /// The whole lane: pre-take points outside the recorded span plus the recorded ones.
    fn lane(&self) -> Vec<ControllerPoint> {
        let (Some(first), Some(last)) = (self.recorded.first(), self.recorded.last()) else {
            return self.original.clone();
        };
        let mut points: Vec<ControllerPoint> = self
            .original
            .iter()
            .copied()
            .filter(|p| p.tick < first.tick || p.tick > last.tick)
            .chain(self.recorded.iter().copied())
            .collect();
        points.sort_by_key(|p| p.tick);
        points
    }
}

#[derive(Default)]
pub(crate) struct ControllerTake {
    lanes: HashMap<(TrackId, ControllerKind), Recording>,
}

/// A controller event already converted to take clip time.
pub(crate) struct Played {
    pub kind: ControllerKind,
    pub value: f32,
    /// Tick inside the clip content.
    pub tick: u32,
    pub pass: u64,
    /// Song seconds (for thinning by time).
    pub seconds: f64,
}

impl ControllerTake {
    pub fn clear(&mut self) {
        self.lanes.clear();
    }

    /// A controller was played on track `track`, which writes into `take`.
    pub fn observe(&mut self, track: TrackId, take: &Take, played: &Played) {
        let recording = self.lanes.entry((track, played.kind)).or_insert_with(|| Recording {
            clip: take.clip,
            original: take.controllers.iter().find(|lane| lane.kind == played.kind).map(|lane| lane.points.clone()).unwrap_or_default(),
            pass: played.pass,
            recorded: Vec::new(),
            last_kept: None,
            pending: None,
            dirty: false,
        });
        if played.pass > recording.pass {
            recording.pass = played.pass;
            recording.recorded.clear();
            recording.pending = None;
            recording.last_kept = None;
            recording.dirty = true;
        }
        let point = ControllerPoint { tick: played.tick, value: played.value };
        match recording.last_kept {
            None => recording.keep(point, played.seconds),
            Some((_, value)) if value == played.value => recording.pending = None,
            Some((seconds, value)) if (played.value - value).abs() > MIN_STEP || played.seconds - seconds > MIN_GAP_SECONDS => {
                recording.keep(point, played.seconds);
            }
            Some(_) => recording.pending = Some(point),
        }
    }

    /// Lanes to write to the project: `(clip, kind, points)`. `flush` means the take ended: deferred small changes are
    /// kept.
    pub fn take_changes(&mut self, flush: bool) -> Vec<(ClipId, ControllerKind, Vec<ControllerPoint>)> {
        let mut changes = Vec::new();
        for ((_, kind), recording) in &mut self.lanes {
            if flush && let Some(point) = recording.pending {
                let seconds = recording.last_kept.map_or(0.0, |(s, _)| s);
                recording.keep(point, seconds);
            }
            if std::mem::take(&mut recording.dirty) {
                changes.push((recording.clip, *kind, recording.lane()));
            }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::ControllerLane;

    const CC: ControllerKind = ControllerKind::Cc { number: 1 };

    fn take(original: &[(u32, f32)]) -> Take {
        let points = original.iter().map(|&(tick, value)| ControllerPoint { tick, value }).collect();
        Take::for_tests(7, vec![ControllerLane { kind: CC, points }])
    }

    fn play(kind: ControllerKind, value: f32, tick: u32, seconds: f64) -> Played {
        Played { kind, value, tick, pass: 0, seconds }
    }

    fn points(changes: &[(ClipId, ControllerKind, Vec<ControllerPoint>)]) -> Vec<(u32, f32)> {
        changes[0].2.iter().map(|p| (p.tick, p.value)).collect()
    }

    #[test]
    fn dense_small_steps_are_thinned_but_the_last_value_survives() {
        let take = take(&[]);
        let mut cc = ControllerTake::default();
        // The 14-bit wheel drifts in small steps every 3 ms: below a CC step, so one point per a bit over 10 ms.
        for step in 0..10u32 {
            cc.observe(1, &take, &play(ControllerKind::PitchBend, step as f32 * 0.0005, step, f64::from(step) * 0.003));
        }
        let kept: Vec<u32> = points(&cc.take_changes(false)).iter().map(|p| p.0).collect();
        assert_eq!(kept, [0, 4, 8]);
        let flushed = points(&cc.take_changes(true));
        assert_eq!(flushed.last(), Some(&(9, 9.0 * 0.0005)), "the last value is kept at the end of the take");
    }

    #[test]
    fn big_jumps_are_kept_and_repeats_are_dropped() {
        let take = take(&[]);
        let mut cc = ControllerTake::default();
        cc.observe(1, &take, &play(CC, 0.0, 0, 0.0));
        cc.observe(1, &take, &play(CC, 0.5, 10, 0.001));
        cc.observe(1, &take, &play(CC, 0.5, 20, 0.5));
        cc.observe(1, &take, &play(CC, 1.0, 30, 0.501));
        assert_eq!(points(&cc.take_changes(true)), [(0, 0.0), (10, 0.5), (30, 1.0)]);
    }

    #[test]
    fn the_recorded_span_replaces_old_points_inside_it_only() {
        let take = take(&[(0, 0.1), (100, 0.2), (900, 0.9)]);
        let mut cc = ControllerTake::default();
        cc.observe(1, &take, &play(CC, 0.5, 50, 0.0));
        cc.observe(1, &take, &play(CC, 0.7, 500, 1.0));
        assert_eq!(points(&cc.take_changes(false)), [(0, 0.1), (50, 0.5), (500, 0.7), (900, 0.9)]);

        // A new pass replaces what the previous one recorded.
        cc.observe(1, &take, &Played { pass: 1, ..play(CC, 0.3, 700, 5.0) });
        assert_eq!(points(&cc.take_changes(false)), [(0, 0.1), (100, 0.2), (700, 0.3), (900, 0.9)]);
        assert!(cc.take_changes(false).is_empty(), "nothing new, nothing to write");
    }
}
