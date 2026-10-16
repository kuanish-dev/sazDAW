//! Lanes the engine must not play now: their params are being written live (`AutomationHold`). The control side (the
//! automation writer) replaces the set, the audio thread reads it. Atomics only: one slot is one lane id.

use std::sync::atomic::{AtomicU64, Ordering};

use daw_contracts::{LaneId, MAX_HELD_LANES};

/// An empty slot; a lane id is stored as `id + 1`, so lane 0 is a valid id too.
const EMPTY: u64 = 0;

pub struct HeldLanes {
    slots: Vec<AtomicU64>,
}

impl HeldLanes {
    pub fn new() -> Self {
        Self { slots: (0..MAX_HELD_LANES).map(|_| AtomicU64::new(EMPTY)).collect() }
    }

    /// Control side: replaces the whole set; anything beyond the capacity is dropped.
    pub fn set(&self, lanes: &[LaneId]) {
        let mut lanes = lanes.iter();
        for slot in &self.slots {
            slot.store(lanes.next().map_or(EMPTY, |&id| u64::from(id) + 1), Ordering::Relaxed);
        }
    }

    /// Audio thread: whether the lane is held.
    pub fn contains(&self, lane: LaneId) -> bool {
        let wanted = u64::from(lane) + 1;
        self.slots.iter().any(|slot| slot.load(Ordering::Relaxed) == wanted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_is_replaced_whole() {
        let held = HeldLanes::new();
        held.set(&[0, 7]);
        assert!(held.contains(0) && held.contains(7) && !held.contains(1));
        held.set(&[3]);
        assert!(held.contains(3) && !held.contains(7) && !held.contains(0));
        held.set(&[]);
        assert!(!held.contains(3));
    }
}
