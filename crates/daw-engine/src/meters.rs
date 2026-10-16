//! Level meters: slots where the graph mixer reports the peaks of tracks, buses and the master and the gain reduction
//! of effects, and the control side collects them (`EngineStats::meters`, `EngineStats::gain_reductions`). Atomics
//! only: the audio thread writes, the control side reads and resets.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use daw_contracts::{GainReduction, MAX_NODES, Meter, NodeId};

/// The most nodes that get meters.
pub const MAX_METERS: usize = MAX_NODES;
const NO_NODE: u32 = u32::MAX;

struct MeterSlot {
    node: AtomicU32,
    left: AtomicU32,
    right: AtomicU32,
}

/// The gain reduction of one effect, by the effect's fixed position in the mixer effect store.
struct ReductionSlot {
    node: AtomicU32,
    db: AtomicU32,
    /// Something was reported since the last read; a bypassed or removed effect stops reporting and drops out.
    reported: AtomicBool,
}

pub struct Meters {
    slots: Vec<MeterSlot>,
    reductions: Vec<ReductionSlot>,
}

impl Meters {
    pub fn new() -> Self {
        let slots = (0..MAX_METERS)
            .map(|_| MeterSlot { node: AtomicU32::new(NO_NODE), left: AtomicU32::new(0), right: AtomicU32::new(0) })
            .collect();
        let reductions = (0..MAX_NODES)
            .map(|_| ReductionSlot { node: AtomicU32::new(NO_NODE), db: AtomicU32::new(0), reported: AtomicBool::new(false) })
            .collect();
        Self { slots, reductions }
    }

    /// Binds node `node` to slot `slot`.
    pub fn set_node(&self, slot: usize, node: NodeId) {
        if let Some(meter) = self.slots.get(slot) {
            meter.node.store(node, Ordering::Relaxed);
        }
    }

    /// Frees the slots starting at `first`.
    pub fn clear_from(&self, first: usize) {
        for meter in self.slots.iter().skip(first) {
            meter.node.store(NO_NODE, Ordering::Relaxed);
        }
    }

    /// Accumulates the slot peak (the maximum since the last read). Peaks are positive, so the bit order of f32
    /// matches the order of values.
    pub fn report_peak(&self, slot: usize, left: f32, right: f32) {
        if let Some(meter) = self.slots.get(slot) {
            meter.left.fetch_max(left.to_bits(), Ordering::Relaxed);
            meter.right.fetch_max(right.to_bits(), Ordering::Relaxed);
        }
    }

    /// The peaks of the bound slots since the last read; reading resets them.
    pub fn take(&self) -> Vec<Meter> {
        self.slots
            .iter()
            .filter_map(|slot| {
                let node = slot.node.load(Ordering::Relaxed);
                (node != NO_NODE).then(|| Meter {
                    node,
                    peak_left: f32::from_bits(slot.left.swap(0, Ordering::Relaxed)),
                    peak_right: f32::from_bits(slot.right.swap(0, Ordering::Relaxed)),
                })
            })
            .collect()
    }

    /// Accumulates the deepest gain reduction of the effect at store position `slot` (node `node`). Like peaks, the
    /// reduction is non-negative, so the bit order of f32 matches the order of values.
    pub fn report_reduction(&self, slot: usize, node: NodeId, db: f32) {
        if let Some(meter) = self.reductions.get(slot) {
            meter.node.store(node, Ordering::Relaxed);
            meter.db.fetch_max(db.max(0.0).to_bits(), Ordering::Relaxed);
            meter.reported.store(true, Ordering::Relaxed);
        }
    }

    /// The reductions reported since the last read; reading resets them.
    pub fn take_reductions(&self) -> Vec<GainReduction> {
        self.reductions
            .iter()
            .filter(|slot| slot.reported.swap(false, Ordering::Relaxed))
            .map(|slot| GainReduction { node: slot.node.load(Ordering::Relaxed), db: f32::from_bits(slot.db.swap(0, Ordering::Relaxed)) })
            .collect()
    }
}
