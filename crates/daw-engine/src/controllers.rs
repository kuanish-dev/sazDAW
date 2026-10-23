//! The segment controller schedule: the points of note clip controller lanes (pitch bend, CC, aftertouch) in the
//! segment tick window, and the chase after start and seek — the values held at the new position, so that jumping into
//! the middle of a bend sounds the same as playing from the start. The content repeats, like notes.

use daw_contracts::{ClipContent, ControllerKind, ControllerLane, EngineSnapshot, TrackId};

const MAX_EVENTS: usize = 8192;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ControllerEvent {
    pub tick: i64,
    pub track: TrackId,
    pub kind: ControllerKind,
    pub value: f32,
    /// A chase value: at the same tick it goes before a window point.
    chased: bool,
}

pub struct ControllerSchedule {
    events: Vec<ControllerEvent>,
}

/// Where a note clip lies on the timeline (like `notes::Placement`).
#[derive(Clone, Copy)]
struct Placement {
    track: TrackId,
    start: i64,
    length: i64,
    phase: i64,
    loop_ticks: i64,
}

impl ControllerSchedule {
    pub fn new() -> Self {
        Self { events: Vec::with_capacity(MAX_EVENTS) }
    }

    pub fn events(&self) -> &[ControllerEvent] {
        &self.events
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Events of the tick window `[from, to)` in time order. With `chase`, also the values held at `from` (the last
    /// point before `from` in every clip under `from`), at tick `from`.
    pub fn collect(&mut self, snapshot: &EngineSnapshot, from: i64, to: i64, chase: bool) {
        self.events.clear();
        for clip in &snapshot.project.clips {
            let ClipContent::Notes(content) = &clip.content else {
                continue;
            };
            if content.controllers.is_empty() || content.loop_ticks == 0 {
                continue;
            }
            let place = Placement {
                track: clip.track,
                start: i64::from(clip.start_tick),
                length: i64::from(clip.length_ticks),
                phase: i64::from(clip.offset_ticks),
                loop_ticks: i64::from(content.loop_ticks),
            };
            for lane in &content.controllers {
                if chase {
                    self.chase(lane, place, from);
                }
                self.collect_lane(lane, place, from, to);
            }
        }
        self.events.sort_unstable_by_key(|e| (e.tick, !e.chased));
    }

    fn push(&mut self, event: ControllerEvent) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }

    /// The points of one lane in the window `[from, to)` over all content repeats inside the clip.
    fn collect_lane(&mut self, lane: &ControllerLane, place: Placement, from: i64, to: i64) {
        let Placement { track, start, length, phase, loop_ticks } = place;
        for point in &lane.points {
            let Some(first) = first_occurrence(point.tick, phase, loop_ticks) else {
                continue;
            };
            let behind = from - start - first;
            let mut pass = if behind > 0 { (behind + loop_ticks - 1) / loop_ticks } else { 0 };
            loop {
                let local = first + pass * loop_ticks;
                let tick = start + local;
                if local >= length || tick >= to {
                    break;
                }
                self.push(ControllerEvent { tick, track, kind: lane.kind, value: point.value, chased: false });
                pass += 1;
            }
        }
    }

    /// The lane value held at `at` inside the clip: the last occurrence of a point strictly before `at`
    /// (a point exactly at `at` comes from the window).
    fn chase(&mut self, lane: &ControllerLane, place: Placement, at: i64) {
        let Placement { track, start, length, phase, loop_ticks } = place;
        let local_at = at - start;
        if local_at <= 0 || local_at >= length {
            return;
        }
        let mut held: Option<(i64, f32)> = None;
        for point in &lane.points {
            let Some(first) = first_occurrence(point.tick, phase, loop_ticks) else {
                continue;
            };
            if first >= local_at {
                continue;
            }
            let last = first + (local_at - 1 - first) / loop_ticks * loop_ticks;
            if held.is_none_or(|(when, _)| last > when) {
                held = Some((last, point.value));
            }
        }
        if let Some((_, value)) = held {
            self.push(ControllerEvent { tick: at, track, kind: lane.kind, value, chased: true });
        }
    }
}

/// Where a content point first sounds inside the clip (with the phase); a point beyond the content does not sound.
fn first_occurrence(tick: u32, phase: i64, loop_ticks: i64) -> Option<i64> {
    let tick = i64::from(tick);
    (tick < loop_ticks).then(|| (tick - phase).rem_euclid(loop_ticks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{Clip, ControllerPoint, NoteContent, Project};

    fn snapshot(start: u32, length: u32, phase: u32, loop_ticks: u32, points: &[(u32, f32)]) -> EngineSnapshot {
        let mut project = Project::empty();
        let lane = ControllerLane {
            kind: ControllerKind::PitchBend,
            points: points.iter().map(|&(tick, value)| ControllerPoint { tick, value }).collect(),
        };
        let content = NoteContent { loop_ticks, auto_length: false, controllers: vec![lane], ..NoteContent::empty() };
        project.clips.push(Clip {
            id: 1,
            track: 7,
            start_tick: start,
            length_ticks: length,
            content: ClipContent::Notes(content),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: phase,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        });
        EngineSnapshot { version: 1, project, order: Vec::new(), samples: Default::default(), clip_audio: Default::default() }
    }

    fn collect(snapshot: &EngineSnapshot, from: i64, to: i64, chase: bool) -> Vec<(i64, f32)> {
        let mut schedule = ControllerSchedule::new();
        schedule.collect(snapshot, from, to, chase);
        schedule.events().iter().map(|e| (e.tick, e.value)).collect()
    }

    #[test]
    fn points_repeat_with_the_content_and_stop_at_the_clip_end() {
        let s = snapshot(1000, 2500, 0, 960, &[(0, -1.0), (480, 0.5)]);
        assert_eq!(collect(&s, 0, 10_000, false), [(1000, -1.0), (1480, 0.5), (1960, -1.0), (2440, 0.5), (2920, -1.0), (3400, 0.5)]);
        assert_eq!(collect(&s, 1480, 1960, false), [(1480, 0.5)], "window: start included, end not");
    }

    #[test]
    fn chase_sends_the_value_held_at_the_new_position_before_points_of_the_window() {
        let s = snapshot(0, 10_000, 0, 960, &[(100, 0.25), (500, 0.75)]);
        assert_eq!(collect(&s, 700, 800, true), [(700, 0.75)]);
        assert_eq!(collect(&s, 1000, 1050, true), [(1000, 0.75)], "on the second pass, before the first point, the last one holds");
        assert_eq!(collect(&s, 500, 600, true), [(500, 0.25), (500, 0.75)], "first what was held, then the point");
        assert_eq!(collect(&s, 50, 60, true), [], "nothing to hold before the clip's first point");
    }

    #[test]
    fn chase_respects_phase_and_ignores_positions_outside_the_clip() {
        let s = snapshot(2000, 1000, 400, 960, &[(100, 0.25), (500, 0.75)]);
        assert_eq!(collect(&s, 2300, 2301, true), [(2300, 0.75)]);
        assert_eq!(collect(&s, 2700, 2701, true), [(2700, 0.25)]);
        assert!(collect(&s, 1500, 1501, true).is_empty(), "before the clip");
        assert!(collect(&s, 3500, 3501, true).is_empty(), "after the clip");
    }
}
