//! The track instrument rack on the audio thread side: holds the instruments, binds them to graph nodes, passes them
//! notes and params and plays them into the node buffers. The capacity is preallocated.

use daw_contracts::{
    AudioBlock, BlockTiming, CC_MODULATION, CC_SUSTAIN, ControllerKind, EngineSnapshot, Instrument, StreamConfig, TrackId,
};

use crate::mixer::GraphMixer;
use crate::params;

const MAX_INSTRUMENTS: usize = 128;

struct InstrumentSlot {
    track: TrackId,
    instrument: Box<dyn Instrument>,
    /// The track's `Instrument` node in the mixer; `None` means there is no node and the instrument plays into nowhere.
    node: Option<usize>,
    /// Arrived after the previous snapshot: it needs all params, not only the changed ones.
    fresh: bool,
    /// Which of the resettable controllers the instrument got since the last reset.
    touched: Touched,
}

/// Controllers reset to 0 on stop and seek (like MIDI "Reset All Controllers"): pitch wheel, pressure, modulation and
/// sustain. Only touched ones are reset, so the instrument gets no extra messages.
#[derive(Clone, Copy, Default)]
struct Touched {
    bend: bool,
    aftertouch: bool,
    modulation: bool,
    sustain: bool,
}

impl Touched {
    fn flag(&mut self, kind: ControllerKind) -> Option<&mut bool> {
        match kind {
            ControllerKind::PitchBend => Some(&mut self.bend),
            ControllerKind::Aftertouch => Some(&mut self.aftertouch),
            ControllerKind::Cc { number: CC_MODULATION } => Some(&mut self.modulation),
            ControllerKind::Cc { number: CC_SUSTAIN } => Some(&mut self.sustain),
            ControllerKind::Cc { .. } => None,
        }
    }
}

const RESETTABLE: [ControllerKind; 4] = [
    ControllerKind::PitchBend,
    ControllerKind::Aftertouch,
    ControllerKind::Cc { number: CC_MODULATION },
    ControllerKind::Cc { number: CC_SUSTAIN },
];

pub struct InstrumentRack {
    slots: Vec<InstrumentSlot>,
}

impl InstrumentRack {
    pub fn new() -> Self {
        Self { slots: Vec::with_capacity(MAX_INSTRUMENTS) }
    }

    /// Sets the track instrument. Returns the previous one or one that did not fit: it must be freed outside the audio thread.
    pub fn add(&mut self, track: TrackId, instrument: Box<dyn Instrument>) -> Option<Box<dyn Instrument>> {
        let slot = InstrumentSlot { track, instrument, node: None, fresh: true, touched: Touched::default() };
        if let Some(existing) = self.find_mut(track) {
            return Some(std::mem::replace(existing, slot).instrument);
        }
        if self.slots.len() < MAX_INSTRUMENTS {
            self.slots.push(slot);
            return None;
        }
        Some(slot.instrument)
    }

    pub fn remove(&mut self, track: TrackId) -> Option<Box<dyn Instrument>> {
        let index = self.slots.iter().position(|s| s.track == track)?;
        Some(self.slots.swap_remove(index).instrument)
    }

    pub fn prepare(&mut self, config: StreamConfig) {
        for slot in &mut self.slots {
            slot.instrument.prepare(config);
        }
    }

    fn find_mut(&mut self, track: TrackId) -> Option<&mut InstrumentSlot> {
        self.slots.iter_mut().find(|s| s.track == track)
    }

    /// Presses (`velocity > 0`) or releases a note on the track; `false` means the track has no instrument.
    pub fn note(&mut self, track: TrackId, pitch: u8, velocity: f32) -> bool {
        let Some(slot) = self.find_mut(track) else {
            return false;
        };
        if velocity > 0.0 {
            slot.instrument.note_on(pitch, velocity);
        } else {
            slot.instrument.note_off(pitch);
        }
        true
    }

    /// A controller for the track instrument; `false` means the track has no instrument.
    pub fn controller(&mut self, track: TrackId, kind: ControllerKind, value: f32) -> bool {
        let Some(slot) = self.find_mut(track) else {
            return false;
        };
        if let Some(flag) = slot.touched.flag(kind) {
            *flag = true;
        }
        slot.instrument.controller(kind, value);
        true
    }

    /// Resets the touched pitch wheel, pressure, modulation and sustain of all instruments to 0.
    pub fn reset_controllers(&mut self) {
        for slot in &mut self.slots {
            for kind in RESETTABLE {
                if let Some(flag) = slot.touched.flag(kind)
                    && std::mem::take(flag)
                {
                    slot.instrument.controller(kind, 0.0);
                }
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for slot in &mut self.slots {
            slot.instrument.all_notes_off();
        }
    }

    /// The segment time for all instruments (before the first `render` of the segment).
    pub fn set_timing(&mut self, timing: &BlockTiming) {
        for slot in &mut self.slots {
            slot.instrument.set_timing(timing);
        }
    }

    /// The latency of the track instrument (latency compensation); no instrument means 0.
    pub fn latency(&self, track: TrackId) -> u32 {
        self.slots.iter().find(|s| s.track == track).map_or(0, |s| s.instrument.latency_samples())
    }

    pub fn set_param(&mut self, track: TrackId, name: &str, value: f32) {
        if let Some(slot) = self.find_mut(track) {
            slot.instrument.set_param(name, value);
        }
    }

    /// Binds the instruments to the nodes of the new snapshot and sends them the params changed since `previous`
    /// (all of them for new instruments and without `previous`).
    pub fn apply_snapshot(&mut self, snapshot: &EngineSnapshot, previous: Option<&EngineSnapshot>, mixer: &GraphMixer) {
        for slot in &mut self.slots {
            slot.node = mixer.instrument_node(slot.track);
            let fresh = std::mem::take(&mut slot.fresh);
            let Some(spec) = snapshot.project.track(slot.track).and_then(|t| t.instrument.as_ref()) else {
                continue;
            };
            let before = previous
                .and_then(|p| p.project.track(slot.track))
                .and_then(|t| t.instrument.as_ref())
                .filter(|old| !fresh && old.kind == spec.kind)
                .map(|old| &old.params);
            for (name, value) in params::changed(&spec.params, before) {
                slot.instrument.set_param(name, value);
            }
        }
    }

    /// Plays all instruments in frames `[from, to)` of their node buffers.
    pub fn render(&mut self, mixer: &mut GraphMixer, from: usize, to: usize) {
        if from >= to {
            return;
        }
        for slot in &mut self.slots {
            let Some(buffer) = slot.node.and_then(|node| mixer.buffer_mut(node, to)) else {
                continue;
            };
            let mut block = AudioBlock::new(&mut buffer[from * 2..], 2);
            slot.instrument.render(&mut block);
        }
    }
}
