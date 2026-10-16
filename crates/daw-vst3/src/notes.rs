//! A fixed-capacity "engine to plugin" note queue and bookkeeping of sounding notes.
//!
//! The instrument's `note_on`/`note_off`/`all_notes_off` only write here (no allocations or calls to the plugin),
//! and at the start of the next `render` the events go to the plugin through `IEventList`. The entity has one job: know
//! which note events are not yet given to the plugin and which notes are sounding now (so `all_notes_off` can stop them).

/// Capacity of the event queue between two blocks.
pub const MAX_EVENTS: usize = 256;
/// How many notes the host counts as sounding at once. A note over the limit is dropped whole (its `NoteOn` never reaches the plugin).
pub const MAX_ACTIVE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEvent {
    On { pitch: u8, velocity: f32, id: i32 },
    Off { pitch: u8, id: i32 },
}

pub struct NoteQueue {
    events: [NoteEvent; MAX_EVENTS],
    len: usize,
    /// Sounding notes: (pitch, `noteId`). Needed so that `NoteOff` carries the same `noteId` as `NoteOn`.
    active: [(u8, i32); MAX_ACTIVE],
    active_len: usize,
    next_id: i32,
}

impl Default for NoteQueue {
    fn default() -> Self {
        Self { events: [NoteEvent::Off { pitch: 0, id: 0 }; MAX_EVENTS], len: 0, active: [(0, 0); MAX_ACTIVE], active_len: 0, next_id: 0 }
    }
}

impl NoteQueue {
    /// A new unique `noteId` (non-negative: -1 in VST3 means "no id").
    fn take_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = if id == i32::MAX { 0 } else { id + 1 };
        id
    }

    fn push(&mut self, event: NoteEvent) -> bool {
        if self.len == MAX_EVENTS {
            return false;
        }
        self.events[self.len] = event;
        self.len += 1;
        true
    }

    /// Queues a `NoteOn`. On overflow (of the queue or the sounding list) and on `NaN` velocity the note
    /// is dropped: the answer is `false` so `BlockFaults` counts the loss.
    pub fn note_on(&mut self, pitch: u8, velocity: f32) -> bool {
        if self.len == MAX_EVENTS || self.active_len == MAX_ACTIVE || velocity.is_nan() {
            return false;
        }
        let id = self.take_id();
        self.push(NoteEvent::On { pitch, velocity: velocity.clamp(0.0, 1.0), id });
        self.active[self.active_len] = (pitch, id);
        self.active_len += 1;
        true
    }

    /// Turns off all sounding notes of this pitch. If the `NoteOff` did not fit into the queue, the note stays sounding
    /// and will be turned off by the next `note_off`/`all_notes_off` (a stuck note is worse than a lost event).
    pub fn note_off(&mut self, pitch: u8) {
        self.release(|p| p == pitch);
    }

    pub fn all_notes_off(&mut self) {
        self.release(|_| true);
    }

    fn release(&mut self, matches: impl Fn(u8) -> bool) {
        let mut kept = 0;
        for index in 0..self.active_len {
            let (pitch, id) = self.active[index];
            if matches(pitch) && self.push(NoteEvent::Off { pitch, id }) {
                continue;
            }
            self.active[kept] = (pitch, id);
            kept += 1;
        }
        self.active_len = kept;
    }

    /// Events accumulated since the previous block.
    pub fn pending(&self) -> &[NoteEvent] {
        &self.events[..self.len]
    }

    /// The events were given to the plugin (or dropped): the queue is free. Sounding notes stay.
    pub fn clear_events(&mut self) {
        self.len = 0;
    }

    /// The plugin was restarted: it forgot all notes, and so did the host.
    pub fn reset(&mut self) {
        self.len = 0;
        self.active_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_off_carries_the_id_of_its_note_on() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 0.8);
        queue.note_on(64, 0.5);
        queue.note_off(60);
        assert_eq!(
            queue.pending(),
            [
                NoteEvent::On { pitch: 60, velocity: 0.8, id: 0 },
                NoteEvent::On { pitch: 64, velocity: 0.5, id: 1 },
                NoteEvent::Off { pitch: 60, id: 0 },
            ]
        );
    }

    #[test]
    fn ids_are_unique_and_never_negative() {
        let mut queue = NoteQueue { next_id: i32::MAX, ..NoteQueue::default() };
        queue.note_on(60, 1.0);
        queue.note_on(60, 1.0);
        let ids: Vec<i32> = queue.pending().iter().map(|e| if let NoteEvent::On { id, .. } = e { *id } else { -1 }).collect();
        assert_eq!(ids, [i32::MAX, 0]);
    }

    #[test]
    fn retriggered_pitch_is_released_with_every_id() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 1.0);
        queue.note_on(60, 1.0);
        queue.clear_events();
        queue.note_off(60);
        assert_eq!(queue.pending(), [NoteEvent::Off { pitch: 60, id: 0 }, NoteEvent::Off { pitch: 60, id: 1 }]);
        queue.clear_events();
        queue.note_off(60);
        assert!(queue.pending().is_empty(), "no second NoteOff needed");
    }

    #[test]
    fn all_notes_off_releases_exactly_the_sounding_notes() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 1.0);
        queue.note_on(62, 1.0);
        queue.note_off(60);
        queue.clear_events();
        queue.all_notes_off();
        assert_eq!(queue.pending(), [NoteEvent::Off { pitch: 62, id: 1 }]);
    }

    #[test]
    fn full_queue_drops_new_notes_without_panicking() {
        let mut queue = NoteQueue::default();
        for _ in 0..MAX_EVENTS {
            assert!(queue.note_on(60, 1.0));
        }
        assert!(!queue.note_on(60, 1.0), "the caller sees the loss");
        assert_eq!(queue.pending().len(), MAX_EVENTS);
        queue.all_notes_off();
        queue.note_off(60);
        assert_eq!(queue.pending().len(), MAX_EVENTS);
    }

    #[test]
    fn dropped_note_off_is_retried_later() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 1.0);
        queue.clear_events();
        for _ in 0..MAX_EVENTS {
            queue.push(NoteEvent::Off { pitch: 1, id: 99 });
        }
        queue.note_off(60);
        queue.clear_events();
        queue.note_off(60);
        assert_eq!(queue.pending(), [NoteEvent::Off { pitch: 60, id: 0 }], "the note did not get stuck");
    }

    #[test]
    fn velocity_is_clamped_and_nan_is_ignored() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 3.0);
        assert!(!queue.note_on(61, f32::NAN));
        assert_eq!(queue.pending(), [NoteEvent::On { pitch: 60, velocity: 1.0, id: 0 }]);
    }

    #[test]
    fn reset_forgets_notes() {
        let mut queue = NoteQueue::default();
        queue.note_on(60, 1.0);
        queue.reset();
        queue.all_notes_off();
        assert!(queue.pending().is_empty());
    }
}
