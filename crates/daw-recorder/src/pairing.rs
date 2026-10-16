//! Press–release pairs: which notes are held now, and finished notes once they are released (or when the take ends
//! with keys still held).

use std::collections::{BTreeMap, HashMap};

use daw_contracts::{NewNote, TrackId};

use crate::shape::{finish_note, pass_of};
use crate::take::TrackTakes;

/// Finished notes per track; each is tagged with the pass it was played on.
pub(crate) type Finished = BTreeMap<TrackId, Vec<(u64, NewNote)>>;

#[derive(Default)]
pub(crate) struct Pairing {
    /// Notes held now: (track, pitch) → (clip time, velocity).
    open: HashMap<(TrackId, u8), (f64, f32)>,
    /// The last seen transport tick: held notes are closed at it after stop.
    last_tick: f64,
}

impl Pairing {
    /// A new take from tick `start`.
    pub fn restart(&mut self, start: f64) {
        self.open.clear();
        self.last_tick = start;
    }

    pub fn observe(&mut self, tick: f64) {
        self.last_tick = self.last_tick.max(tick);
    }

    pub fn last_tick(&self) -> f64 {
        self.last_tick
    }

    /// A note pressed at `relative` (time in the clip content).
    pub fn press(&mut self, track: TrackId, pitch: u8, relative: f64, velocity: f32) {
        self.open.insert((track, pitch), (relative, velocity));
    }

    /// Note (track, pitch) released or pressed again at transport tick `tick`: if it was held, it becomes a finished
    /// note.
    pub fn release(&mut self, track: TrackId, pitch: u8, tick: f64, takes: &TrackTakes, quantize: u32, into: &mut Finished) {
        if let Some(pressed) = self.open.remove(&(track, pitch)) {
            finish((track, pitch), pressed, tick, takes, quantize, into);
        }
    }

    /// The take ended at tick `end`: all held notes are closed there.
    pub fn close_all(&mut self, end: f64, takes: &TrackTakes, quantize: u32) -> Finished {
        let mut finished = Finished::new();
        for (key, pressed) in self.open.drain() {
            finish(key, pressed, end, takes, quantize, &mut finished);
        }
        finished
    }
}

/// A held note (an entry of `Pairing::open`) released at transport tick `end`: adds the finished note with its pass.
fn finish((track, pitch): (TrackId, u8), (start, velocity): (f64, f32), end: f64, takes: &TrackTakes, quantize: u32, into: &mut Finished) {
    let Some(take) = takes.get(track) else {
        return;
    };
    if let Some(note) = finish_note(pitch, velocity, start, end + take.shift, quantize, take.loop_ticks) {
        into.entry(track).or_default().push((pass_of(start, take.loop_ticks), note));
    }
}
