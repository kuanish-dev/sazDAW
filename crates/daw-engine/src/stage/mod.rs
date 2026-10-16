//! The virtual stage on the audio thread (its control side is `StageControl` in `daw-project`): a spatializer voice for
//! each channel standing on the stage (`voice.rs`), the direction cue it gives a sound — the parametric HRTF on
//! headphones, panning on speakers (`hrtf.rs`) — the shared room's diffuse tail (`room.rs`), the geometry they follow
//! (`geometry.rs`) and their small DSP blocks (`dsp.rs`). The mixer runs a channel's voice where its sound
//! leaves its insert path and the room tail on the master's input (`mixer/mod.rs`).
//!
//! Voices live in `MAX_ON_STAGE` slots bound to node ids, so a channel keeps its delay lines and its glide across
//! snapshots. Lines are allocated only by `new`/`prepare`, which run outside the audio callback.

pub(crate) mod dsp;
mod geometry;
mod hrtf;
mod room;
mod voice;

use daw_contracts::{MAX_ON_STAGE, NodeId, Room};

use crate::SEGMENT_FRAMES;
pub use geometry::Spot;
use room::RoomTail;
use voice::{Acoustics, Voice};

/// How much a hard wall sends back at full reverb.
const WALL_REFLECTIVITY: f32 = 0.8;
/// How much of the dry sound feeds the room tail at full reverb: at half reverb the tail sits about 12 dB under the
/// direct sound at the reference distance, so a source becomes as loud in the room as it is directly about 8 m away.
const ROOM_SEND: f32 = 0.4;

struct Slot {
    node: Option<NodeId>,
    /// Kept by the snapshot being bound.
    kept: bool,
    voice: Voice,
}

pub struct Stage {
    slots: Vec<Slot>,
    tail: RoomTail,
    room: Room,
    acoustics: Acoustics,
    /// The room tail's input this segment (stereo, interleaved) and whether any channel fed it.
    room_input: Vec<f32>,
    fed: bool,
}

fn acoustics(room: Room, sample_rate: f32) -> Acoustics {
    let shape = geometry::RoomShape::of(room.size);
    Acoustics {
        shape,
        listening: room.listening,
        reflectivity: WALL_REFLECTIVITY * room.reverb,
        send: ROOM_SEND * room.reverb,
        wall: dsp::one_pole(shape.wall_hz, sample_rate),
        sample_rate,
    }
}

impl Stage {
    /// The stage for this sample rate (allocates the voices' lines and the tail: only outside the audio thread).
    pub fn new(sample_rate: f32) -> Self {
        let room = Room::default();
        let acoustics = acoustics(room, sample_rate);
        Self {
            slots: (0..MAX_ON_STAGE).map(|_| Slot { node: None, kept: false, voice: Voice::new(sample_rate) }).collect(),
            tail: RoomTail::new(sample_rate, &acoustics.shape),
            room,
            acoustics,
            room_input: vec![0.0; SEGMENT_FRAMES * 2],
            fed: false,
        }
    }

    /// A new sample rate: the lines are made anew; the channels keep their slots and start again from their places.
    pub fn prepare(&mut self, sample_rate: f32) {
        let nodes: Vec<Option<NodeId>> = self.slots.iter().map(|slot| slot.node).collect();
        let room = self.room;
        *self = Self::new(sample_rate);
        for (slot, node) in self.slots.iter_mut().zip(nodes) {
            slot.node = node;
        }
        self.set_room(room);
    }

    pub fn set_room(&mut self, room: Room) {
        if room == self.room {
            return;
        }
        self.room = room;
        self.acoustics = acoustics(room, self.acoustics.sample_rate);
        self.tail.set_shape(&self.acoustics.shape);
    }

    /// Binding a snapshot, step 1: the voices of the channels still on the stage are kept (`keep` each of them), then
    /// `release_others` frees the rest, so a channel leaving gives its voice to one arriving in the same snapshot.
    pub fn keep(&mut self, node: NodeId) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.node == Some(node)) {
            slot.kept = true;
        }
    }

    pub fn release_others(&mut self) {
        for slot in &mut self.slots {
            if !std::mem::take(&mut slot.kept) {
                slot.node = None;
            }
        }
    }

    /// Binding a snapshot, step 2: the voice of a channel on the stage — its own if it had one, otherwise a free one,
    /// reset. `None` if all are taken (the project allows no more than `MAX_ON_STAGE`).
    pub fn bind(&mut self, node: NodeId) -> Option<usize> {
        if let Some(index) = self.slots.iter().position(|slot| slot.node == Some(node)) {
            return Some(index);
        }
        let index = self.slots.iter().position(|slot| slot.node.is_none())?;
        let slot = &mut self.slots[index];
        slot.node = Some(node);
        slot.voice.reset(self.acoustics.sample_rate);
        Some(index)
    }

    /// A new segment: the room tail's input starts empty.
    pub fn clear(&mut self, frames: usize) {
        self.room_input[..frames * 2].fill(0.0);
        self.fed = false;
    }

    /// Replaces the channel's sound in `buffer` with what the listener hears from `spot`; `feeds_room` — the sound
    /// reaches the master, so it also plays in the room tail (a channel routed nowhere stays silent in the room too).
    pub fn process(&mut self, slot: usize, buffer: &mut [f32], spot: Spot, feeds_room: bool) {
        let Some(slot) = self.slots.get_mut(slot) else {
            return;
        };
        let feeds = feeds_room && self.acoustics.send > 0.0;
        self.fed |= feeds;
        let room = feeds.then(|| &mut self.room_input[..buffer.len()]);
        slot.voice.process(buffer, spot, &self.acoustics, room);
    }

    /// Adds the room tail to the master's input (before its fader).
    pub fn render_room(&mut self, buffer: &mut [f32]) {
        self.tail.process(&self.room_input[..buffer.len()], self.fed, buffer);
    }
}

#[cfg(test)]
mod tests;
