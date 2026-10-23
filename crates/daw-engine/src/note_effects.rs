//! Track MIDI effects on the audio thread side: "run the segment notes through the track chains before the instruments".
//!
//! The conductor collects the segment notes (`SegmentNotes`): live ones at frame 0, clip notes at their frames. Notes of
//! tracks without a chain go to the instruments as they are; the others pass through the chain effects in order
//! (bypassed ones are skipped). To avoid hanging notes, the rack remembers which keys are held at the chain input and
//! which notes the chain pressed on the instrument: when the chain or the bypass changes, it releases what was pressed,
//! resets the effects and feeds the held keys again (remove the arpeggiator while holding a chord, and the chord
//! sounds). Stop and seek reset everything (`release_all`): `Instrument::all_notes_off` silences what sounds.
//! Capacities are preallocated.

use daw_contracts::{BlockTiming, EngineSnapshot, MAX_NOTE_EFFECTS, NoteBuffer, NoteEffect, NoteEffectId, NoteEvent, TrackId};

use crate::params;

const MAX_CHAINS: usize = 128;
const MAX_SEGMENT_NOTES: usize = 8192;

/// A track chain as the control side sent it.
pub type Units = Vec<(NoteEffectId, Box<dyn NoteEffect>)>;

/// A segment note for the track instrument; `seq` is the order of adding (kept within one frame).
#[derive(Clone, Copy)]
pub struct TimedNote {
    pub track: TrackId,
    pub event: NoteEvent,
    seq: usize,
}

/// The segment notes: played ones (`push`) and the ones the instruments will get (`outgoing`, by frame).
pub struct SegmentNotes {
    incoming: Vec<TimedNote>,
    outgoing: Vec<TimedNote>,
}

impl SegmentNotes {
    pub fn new() -> Self {
        Self { incoming: Vec::with_capacity(MAX_SEGMENT_NOTES), outgoing: Vec::with_capacity(MAX_SEGMENT_NOTES) }
    }

    pub fn clear(&mut self) {
        self.incoming.clear();
        self.outgoing.clear();
    }

    /// A played note of the segment; notes are added in non-decreasing frame order. Beyond the capacity it is lost (as in the note schedule).
    pub fn push(&mut self, track: TrackId, event: NoteEvent) {
        if self.incoming.len() < MAX_SEGMENT_NOTES {
            self.incoming.push(TimedNote { track, event, seq: 0 });
        }
    }

    fn send(&mut self, track: TrackId, event: NoteEvent) {
        if self.outgoing.len() < MAX_SEGMENT_NOTES {
            let seq = self.outgoing.len();
            self.outgoing.push(TimedNote { track, event, seq });
        }
    }

    /// The notes for the instruments by frame.
    pub fn outgoing(&self) -> &[TimedNote] {
        &self.outgoing
    }
}

struct Chain {
    track: TrackId,
    units: Units,
    bypass: [bool; MAX_NOTE_EFFECTS],
    /// The chain has just arrived: it needs all params.
    fresh: bool,
    /// The chain or the bypass changed: release what was pressed, reset the effects and feed the held keys again.
    flush: bool,
    /// The velocity of the held input keys (0 means not held).
    held: [f32; 128],
    /// What the chain pressed on the instrument and has not released yet.
    sounding: [bool; 128],
}

impl Chain {
    fn new(track: TrackId, units: Units) -> Self {
        Self { track, units, bypass: [false; MAX_NOTE_EFFECTS], fresh: true, flush: true, held: [0.0; 128], sounding: [false; 128] }
    }
}

pub struct NoteEffectRack {
    chains: Vec<Chain>,
    buffer: NoteBuffer,
    scratch: NoteBuffer,
}

impl NoteEffectRack {
    pub fn new() -> Self {
        Self { chains: Vec::with_capacity(MAX_CHAINS), buffer: NoteBuffer::new(), scratch: NoteBuffer::new() }
    }

    /// Sets the track chain (an empty one removes it: notes go past, what the chain pressed is released). Returns the
    /// previous effects or the ones that did not fit: they must be freed outside the audio thread.
    pub fn set(&mut self, track: TrackId, units: Units) -> Option<Units> {
        if let Some(chain) = self.chains.iter_mut().find(|c| c.track == track) {
            chain.fresh = true;
            chain.flush = true;
            chain.bypass = [false; MAX_NOTE_EFFECTS];
            return Some(std::mem::replace(&mut chain.units, units));
        }
        if units.is_empty() || self.chains.len() >= MAX_CHAINS {
            return Some(units);
        }
        self.chains.push(Chain::new(track, units));
        None
    }

    /// Stop or seek: the effects forget everything, what sounds is already silenced at the instruments.
    pub fn release_all(&mut self) {
        for chain in &mut self.chains {
            for (_, effect) in &mut chain.units {
                effect.reset();
            }
            chain.held = [0.0; 128];
            chain.sounding = [false; 128];
            chain.flush = false;
        }
    }

    /// The params changed since `previous` (all of them for new chains) and the effect bypass from the snapshot; a
    /// bypass change is a reason to release what the chain pressed.
    pub fn apply_snapshot(&mut self, snapshot: &EngineSnapshot, previous: Option<&EngineSnapshot>) {
        for chain in &mut self.chains {
            let fresh = std::mem::take(&mut chain.fresh);
            let Some(track) = snapshot.project.track(chain.track) else { continue };
            let before = previous.filter(|_| !fresh).and_then(|p| p.project.track(chain.track));
            for (index, (id, effect)) in chain.units.iter_mut().enumerate() {
                let Some(spec) = track.note_effects.iter().find(|s| s.id == *id) else { continue };
                let old = before.and_then(|t| t.note_effects.iter().find(|s| s.id == *id)).map(|s| &s.params);
                for (name, value) in params::changed(&spec.params, old) {
                    effect.set_param(name, value);
                }
                if let Some(bypass) = chain.bypass.get_mut(index)
                    && *bypass != spec.bypass
                {
                    *bypass = spec.bypass;
                    chain.flush = true;
                }
            }
        }
    }

    /// Runs the played segment notes through the chains: `notes.outgoing` gets what the instruments receive, by frame.
    pub fn process(&mut self, notes: &mut SegmentNotes, timing: &BlockTiming) {
        for index in 0..notes.incoming.len() {
            let note = notes.incoming[index];
            if !self.chains.iter().any(|c| c.track == note.track) {
                notes.send(note.track, note.event);
            }
        }
        for chain in &mut self.chains {
            self.buffer.clear();
            if std::mem::take(&mut chain.flush) {
                flush(chain, notes, &mut self.buffer);
            }
            for note in notes.incoming.iter().filter(|n| n.track == chain.track) {
                chain.held[usize::from(note.event.pitch & 0x7f)] = note.event.velocity;
                self.buffer.push(note.event);
            }
            for (index, (_, effect)) in chain.units.iter_mut().enumerate() {
                if chain.bypass.get(index).copied().unwrap_or(false) {
                    continue;
                }
                self.scratch.clear();
                effect.process(self.buffer.events(), timing, &mut self.scratch);
                std::mem::swap(&mut self.buffer, &mut self.scratch);
            }
            for &event in self.buffer.events() {
                chain.sounding[usize::from(event.pitch & 0x7f)] = event.is_on();
                notes.send(chain.track, event);
            }
        }
        self.chains.retain(|c| !c.units.is_empty());
        notes.outgoing.sort_unstable_by_key(|n| (n.event.frame, n.seq));
    }
}

/// Releases what the chain pressed (frame 0), resets its effects and feeds the held keys again into `input`.
fn flush(chain: &mut Chain, notes: &mut SegmentNotes, input: &mut NoteBuffer) {
    for pitch in 0..=127u8 {
        if std::mem::take(&mut chain.sounding[usize::from(pitch)]) {
            notes.send(chain.track, NoteEvent::off(0, pitch));
        }
    }
    for (_, effect) in &mut chain.units {
        effect.reset();
    }
    for pitch in 0..=127u8 {
        let velocity = chain.held[usize::from(pitch)];
        if velocity > 0.0 {
            input.push(NoteEvent::on(0, pitch, velocity));
        }
    }
}
