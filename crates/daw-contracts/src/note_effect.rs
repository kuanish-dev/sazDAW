//! MIDI effects of a track: processing notes before the instrument (arpeggiator, chord). The realtime side
//! (`NoteEffect`), the catalog of kinds (`NoteEffectCatalog`) and editing a track's chain in the project
//! (`NoteEffectControl`).
//!
//! The chain sits between the notes and the instrument: clip notes and live notes (keyboard) pass through it in order,
//! and the instrument gets what comes out of the last effect. Recording captures what was played (the chain's input),
//! not its output.

use crate::{BlockTiming, ContractError, KindInfo, NoteEffectId, Params, TrackId};

/// How many MIDI effects one track can hold.
pub const MAX_NOTE_EFFECTS: usize = 8;
/// Capacity of one effect's note buffer per segment (`NoteBuffer`): extra events in a segment are lost.
pub const NOTE_BUFFER_CAPACITY: usize = 1024;

/// A note in a segment: frame within the segment (`0..=timing.frames`), pitch and velocity (`0` is a release,
/// otherwise `0..=1`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    pub frame: usize,
    pub pitch: u8,
    pub velocity: f32,
}

impl NoteEvent {
    pub fn on(frame: usize, pitch: u8, velocity: f32) -> Self {
        Self { frame, pitch, velocity }
    }

    pub fn off(frame: usize, pitch: u8) -> Self {
        Self { frame, pitch, velocity: 0.0 }
    }

    pub fn is_on(&self) -> bool {
        self.velocity > 0.0
    }
}

/// A segment's note buffer, allocated in advance (capacity [`NOTE_BUFFER_CAPACITY`]): it does not grow on the audio
/// thread; an event over capacity is dropped.
pub struct NoteBuffer {
    events: Vec<NoteEvent>,
}

impl NoteBuffer {
    pub fn new() -> Self {
        Self { events: Vec::with_capacity(NOTE_BUFFER_CAPACITY) }
    }

    /// Adds an event; `false` means the buffer is full and the event is lost.
    pub fn push(&mut self, event: NoteEvent) -> bool {
        if self.events.len() >= NOTE_BUFFER_CAPACITY {
            return false;
        }
        self.events.push(event);
        true
    }

    pub fn events(&self) -> &[NoteEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for NoteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// A MIDI effect: turns the notes of a segment into other notes. Created on the control side; once handed to the
/// engine, all methods are called from the realtime thread: no allocations, locks or I/O.
///
/// Contract:
/// * `process` is called for every segment, even without input notes (an arpeggiator plays steps on its own);
/// * output is in non-decreasing frame order within `0..=timing.frames`; on the same frame a note's release comes
///   before its new press;
/// * every note the effect presses is eventually released by the effect itself (on the input release or when its
///   length runs out), except after `reset`: then the engine releases everything that sounds;
/// * time follows `timing`: while playing it is the transport position (steps land on the grid), when stopped the
///   segment's tempo.
pub trait NoteEffect: Send {
    /// Unknown names are ignored.
    fn set_param(&mut self, name: &str, value: f32);
    /// Forget pressed and played notes (stop, seek, bypass, chain change). Emits nothing.
    fn reset(&mut self);
    /// `input` holds the segment's notes in non-decreasing frame order; the result is appended to `output`.
    fn process(&mut self, input: &[NoteEvent], timing: &BlockTiming, output: &mut NoteBuffer);
}

/// Catalog of MIDI effect kinds.
pub trait NoteEffectCatalog: Send + Sync {
    fn kinds(&self) -> Vec<KindInfo>;
    /// Default param values; an unknown kind is `notes::UNKNOWN_NOTE_EFFECT`.
    fn defaults(&self, kind: &str) -> Result<Params, ContractError>;
    /// A new effect with `params` (missing ones take defaults); an unknown kind is `notes::UNKNOWN_NOTE_EFFECT`.
    fn create(&self, kind: &str, params: &Params) -> Result<Box<dyn NoteEffect>, ContractError>;
}

/// A track's MIDI effect chain in the project (`Track::note_effects`). Job: "what happens to a track's notes before
/// the instrument".
///
/// Rules for all methods: an unknown track or effect on it is `ErrorKind::NotFound`, an audio track is
/// `ErrorKind::Invalid`. Every mutating method is one undo step (consecutive `set_param` calls on the same param merge
/// into one).
pub trait NoteEffectControl: Send + Sync {
    /// Appends an effect of kind `kind` to the chain: default params with `params` on top (clamped to the param's
    /// range; an unknown name is `ErrorKind::Invalid`). An unknown kind is `notes::UNKNOWN_NOTE_EFFECT`, more than
    /// [`MAX_NOTE_EFFECTS`] is `ErrorKind::Limit`.
    fn add(&self, track: TrackId, kind: &str, params: Params) -> Result<NoteEffectId, ContractError>;
    fn remove(&self, track: TrackId, effect: NoteEffectId) -> Result<(), ContractError>;
    /// Moves the effect to position `index` in the chain (0 is first, notes reach it earlier; past the end goes to
    /// the end).
    fn move_to(&self, track: TrackId, effect: NoteEffectId, index: usize) -> Result<(), ContractError>;
    /// The value is clamped to the param's range (a choice gets a whole number); an unknown name is
    /// `ErrorKind::Invalid`.
    fn set_param(&self, track: TrackId, effect: NoteEffectId, name: &str, value: f32) -> Result<(), ContractError>;
    /// A bypassed effect passes notes through unchanged.
    fn set_bypass(&self, track: TrackId, effect: NoteEffectId, bypass: bool) -> Result<(), ContractError>;
}
