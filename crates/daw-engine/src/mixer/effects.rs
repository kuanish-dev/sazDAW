//! The mixer effect store: the DSP objects of effect nodes live in slots with fixed positions, a graph node keeps the
//! position of its object. The capacity is `MAX_NODES`, preallocated.

use daw_contracts::{BlockTiming, Effect, MAX_NODES, NodeId, Params, StreamConfig};

use crate::params;

struct EffectSlot {
    node: NodeId,
    effect: Box<dyn Effect>,
    /// Arrived after the previous snapshot: it needs all params, not only the changed ones.
    fresh: bool,
}

pub struct EffectPool {
    slots: Vec<Option<EffectSlot>>,
}

impl EffectPool {
    pub fn new() -> Self {
        Self { slots: Vec::with_capacity(MAX_NODES) }
    }

    /// Sets or replaces a node's effect; the previous one (or one that did not fit) is returned to be freed outside the
    /// audio thread. The effect arrives already prepared (`EngineLink::send` calls `prepare` on the control side):
    /// `prepare` allocates memory (delay, reverb) or restarts the plugin (VST3), which has no place in the audio thread.
    pub fn add(&mut self, node: NodeId, effect: Box<dyn Effect>) -> Option<Box<dyn Effect>> {
        let slot = EffectSlot { node, effect, fresh: true };
        if let Some(existing) = self.slots.iter_mut().flatten().find(|s| s.node == node) {
            return Some(std::mem::replace(existing, slot).effect);
        }
        if let Some(empty) = self.slots.iter_mut().find(|s| s.is_none()) {
            *empty = Some(slot);
        } else if self.slots.len() < MAX_NODES {
            self.slots.push(Some(slot));
        } else {
            return Some(slot.effect);
        }
        None
    }

    pub fn remove(&mut self, node: NodeId) -> Option<Box<dyn Effect>> {
        let position = self.position(node)?;
        self.slots[position].take().map(|s| s.effect)
    }

    pub fn prepare(&mut self, config: StreamConfig) {
        for slot in self.slots.iter_mut().flatten() {
            slot.effect.prepare(config);
        }
    }

    /// The segment time for all effects (plugins that follow the beat).
    pub fn set_timing(&mut self, timing: &BlockTiming) {
        for slot in self.slots.iter_mut().flatten() {
            slot.effect.set_timing(timing);
        }
    }

    /// The effect position of node `node`.
    pub fn position(&self, node: NodeId) -> Option<usize> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.node == node))
    }

    pub fn set_param(&mut self, position: usize, name: &str, value: f32) {
        if let Some(slot) = self.slots.get_mut(position).and_then(Option::as_mut) {
            slot.effect.set_param(name, value);
        }
    }

    /// Sends the effect the params changed since `before` (all of them to a new effect).
    pub fn apply_params(&mut self, position: usize, params: &Params, before: Option<&Params>) {
        let Some(slot) = self.slots.get_mut(position).and_then(Option::as_mut) else {
            return;
        };
        let fresh = std::mem::take(&mut slot.fresh);
        let before = before.filter(|_| !fresh);
        for (name, value) in params::changed(params, before) {
            slot.effect.set_param(name, value);
        }
    }

    /// The effect latency (latency compensation); an empty slot means 0.
    pub fn latency(&self, position: usize) -> u32 {
        self.slots.get(position).and_then(Option::as_ref).map_or(0, |slot| slot.effect.latency_samples())
    }

    /// The band gains of the effect after its last `process` (`Effect::band_gains_db`); empty for an empty slot.
    pub fn band_gains(&self, position: usize) -> &[f32] {
        self.slots.get(position).and_then(Option::as_ref).map_or(&[], |slot| slot.effect.band_gains_db())
    }

    /// Processes the buffer; returns the effect's gain reduction after it (`Effect::gain_reduction_db`).
    pub fn process(&mut self, position: usize, buffer: &mut [f32], sidechain: Option<&[f32]>) -> Option<f32> {
        let slot = self.slots.get_mut(position).and_then(Option::as_mut)?;
        slot.effect.process(buffer, sidechain);
        slot.effect.gain_reduction_db()
    }
}
