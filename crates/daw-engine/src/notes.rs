//! The segment note schedule: from the timeline's note clips, the presses and releases in the segment tick window,
//! plus forced releases of sounding notes that will get no regular release any more (the clip was changed).

use daw_contracts::{ClipContent, EngineSnapshot, Note, TrackId, velocity_level};

const MAX_EVENTS: usize = 8192;
const MAX_ACTIVE: usize = 1024;

#[derive(Clone, Copy, PartialEq)]
pub struct Event {
    pub tick: i64,
    /// `false` is a release; it sorts before a press at the same tick.
    pub on: bool,
    pub track: TrackId,
    pub pitch: u8,
    pub velocity: f32,
    /// For a press: when the note must be released.
    pub off_tick: i64,
}

/// A sounding note: it is released on time even if it is removed from the clip.
#[derive(Clone, Copy)]
struct ActiveNote {
    track: TrackId,
    pitch: u8,
    off_tick: i64,
}

pub struct NoteSchedule {
    events: Vec<Event>,
    active: Vec<ActiveNote>,
}

impl NoteSchedule {
    pub fn new() -> Self {
        Self { events: Vec::with_capacity(MAX_EVENTS), active: Vec::with_capacity(MAX_ACTIVE) }
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// All notes were cut off (stop, seek): no need to remember them any more.
    pub fn forget_active(&mut self) {
        self.active.clear();
    }

    /// Events of the tick window `[from, to)` in order: a release before a press at the same tick.
    pub fn collect(&mut self, snapshot: &EngineSnapshot, from: i64, to: i64) {
        self.events.clear();
        for clip in &snapshot.project.clips {
            let ClipContent::Notes(content) = &clip.content else {
                continue;
            };
            let place = Placement {
                track: clip.track,
                start: i64::from(clip.start_tick),
                length: i64::from(clip.length_ticks),
                phase: i64::from(clip.offset_ticks),
            };
            collect_instance(&mut self.events, &content.notes, i64::from(content.loop_ticks), place, from, to);
        }
        self.collect_forced_offs(from, to);
        self.events.sort_unstable_by_key(|e| (e.tick, e.on, e.track, e.pitch));
        self.events.dedup_by(|a, b| a.tick == b.tick && a.on == b.on && a.track == b.track && a.pitch == b.pitch);
    }

    /// Notes whose release time has come but have no regular event (the clip was changed).
    fn collect_forced_offs(&mut self, from: i64, to: i64) {
        for note in &self.active {
            if note.off_tick < to && self.events.len() < MAX_EVENTS {
                self.events.push(Event {
                    tick: note.off_tick.max(from),
                    on: false,
                    track: note.track,
                    pitch: note.pitch,
                    velocity: 0.0,
                    off_tick: 0,
                });
            }
        }
    }

    /// A note was pressed: remember it to release on time even if it is removed from the clip.
    pub fn note_started(&mut self, event: &Event) {
        if self.active.len() < MAX_ACTIVE {
            self.active.push(ActiveNote { track: event.track, pitch: event.pitch, off_tick: event.off_tick });
        }
    }

    pub fn note_released(&mut self, event: &Event) {
        if let Some(index) = self.active.iter().position(|n| n.track == event.track && n.pitch == event.pitch) {
            self.active.swap_remove(index);
        }
    }
}

/// Where a note clip lies on the timeline.
#[derive(Clone, Copy)]
struct Placement {
    track: TrackId,
    /// The clip start tick.
    start: i64,
    /// The clip length: notes are cut at it.
    length: i64,
    /// Where in the content the clip starts.
    phase: i64,
}

/// The events of one note clip: content `loop_ticks` long repeats until the clip ends.
fn collect_instance(events: &mut Vec<Event>, notes: &[Note], loop_ticks: i64, place: Placement, from: i64, to: i64) {
    let length = loop_ticks;
    if length <= 0 {
        return;
    }
    let Placement { track, start: offset, length: clip_len, phase } = place;
    for note in notes {
        let note_length = i64::from(note.length_ticks);
        let first = (i64::from(note.start_tick) - phase).rem_euclid(length);
        // The first pass whose note end is not before the window start.
        let earliest = from - offset - first - note_length;
        let mut pass = if earliest > 0 { (earliest + length - 1) / length } else { 0 };
        loop {
            let local = first + pass * length;
            let on_tick = offset + local;
            if local >= clip_len || on_tick >= to {
                break;
            }
            let off_tick = offset + (local + note_length).min(clip_len);
            if events.len() + 2 > MAX_EVENTS {
                return;
            }
            if on_tick >= from {
                let velocity = velocity_level(note.velocity);
                events.push(Event { tick: on_tick, on: true, track, pitch: note.pitch, velocity, off_tick });
            }
            if off_tick >= from && off_tick < to {
                events.push(Event { tick: off_tick, on: false, track, pitch: note.pitch, velocity: 0.0, off_tick: 0 });
            }
            pass += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 960 ticks of content; `(pitch, start, length)`.
    fn notes(list: &[(u32, u32, u32)]) -> Vec<Note> {
        list.iter()
            .enumerate()
            .map(|(i, (pitch, start, length))| Note {
                id: i as u32 + 1,
                pitch: *pitch as u8,
                start_tick: *start,
                length_ticks: *length,
                velocity: 100,
            })
            .collect()
    }

    const LONG: i64 = i64::MAX / 4;

    fn collect(n: &[Note], offset: i64, len: i64, phase: i64, from: i64, to: i64) -> Vec<(i64, bool, u8)> {
        let mut events = Vec::new();
        let place = Placement { track: 7, start: offset, length: len, phase };
        collect_instance(&mut events, n, 960, place, from, to);
        events.sort_by_key(|e| (e.tick, e.on));
        events.iter().map(|e| (e.tick, e.on, e.pitch)).collect()
    }

    #[test]
    fn off_arrives_even_when_the_on_was_in_an_earlier_window() {
        let n = notes(&[(60, 100, 300)]);
        assert_eq!(collect(&n, 0, LONG, 0, 350, 500), [(400, false, 60)]);
    }

    #[test]
    fn clip_offset_length_and_phase() {
        let n = notes(&[(60, 0, 400)]);
        // A clip from tick 1000, 300 long: the note is cut at the clip end.
        assert_eq!(collect(&n, 1000, 300, 0, 0, 5000), [(1000, true, 60), (1300, false, 60)]);
        // With a phase shift of 100 the note would start "before" the clip: the first start is in the next pass.
        let events = collect(&n, 1000, 2000, 100, 0, 5000);
        assert_eq!(events.first(), Some(&(1860, true, 60)));
    }

    #[test]
    fn same_pitch_note_spanning_the_whole_content_retriggers_off_before_on() {
        let n = notes(&[(60, 0, 960)]);
        let events = collect(&n, 0, LONG, 0, 900, 1000);
        assert_eq!(events, [(960, false, 60), (960, true, 60)]);
    }

    #[test]
    fn empty_content_or_zero_loop_gives_nothing() {
        assert_eq!(collect(&[], 0, LONG, 0, 0, 5000), [] as [(i64, bool, u8); 0]);
        let mut events = Vec::new();
        let place = Placement { track: 1, start: 0, length: 1000, phase: 0 };
        collect_instance(&mut events, &notes(&[(60, 0, 10)]), 0, place, 0, 5000);
        assert!(events.is_empty());
    }
}
