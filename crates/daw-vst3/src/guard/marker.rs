//! `ProcessMarker`: "mark which plugin instance is processing now", so that after a crash it is clear who is to blame.
//!
//! A plugin instance gets a slot when created (`GuardSlot`: the plugin's kind and name are written to the marker file in
//! advance, on the control side). Before calling the plugin the slot enters processing (`GuardSlot::enter`), after it leaves: this is one
//! atomic store into the mapped file, without allocations, locks or system calls, so it is fit for the audio thread.
//! Slots are taken and freed by lock-free flags: freeing may happen on any thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use daw_contracts::AppLog;

use super::map::{GuardMap, SLOTS};
use crate::journal;

pub struct ProcessMarker {
    /// `None` when the marker does not work (the file did not open; the reason is already logged): plugins work without it.
    map: Option<GuardMap>,
    taken: [AtomicBool; SLOTS],
    log: Arc<dyn AppLog>,
}

impl ProcessMarker {
    pub fn new(map: Option<GuardMap>, log: Arc<dyn AppLog>) -> Self {
        Self { map, taken: std::array::from_fn(|_| AtomicBool::new(false)), log }
    }

    /// A marker in process memory: for sessions without a marker file and for tests.
    pub fn in_memory(log: Arc<dyn AppLog>) -> Self {
        let map =
            GuardMap::anonymous().map_err(|error| journal::warn(&*log, format!("the plugin crash marker does not work: {error}"))).ok();
        Self::new(map, log)
    }

    pub(super) fn set_running(&self, running: bool) {
        if let Some(map) = &self.map {
            map.set_running(running);
        }
    }

    /// A slot for a new instance of plugin `kind` (`name`). If none is free, the instance works without a marker (logged).
    pub fn claim(self: &Arc<Self>, kind: &str, name: &str) -> Option<GuardSlot> {
        let map = self.map.as_ref()?;
        let Some(index) =
            self.taken.iter().position(|flag| flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed).is_ok())
        else {
            journal::warn(&*self.log, format!("crash marker: all {SLOTS} slots are taken, \"{name}\" works without a marker"));
            return None;
        };
        map.write_slot(index, kind, name);
        Some(GuardSlot { marker: self.clone(), index })
    }

    /// Slot `slot` is processing until `Inside` is dropped; does not hold the slot itself (it can be given to an instance).
    pub fn enter(&self, slot: &GuardSlot) -> Inside<'_> {
        if let Some(map) = &self.map {
            map.enter(slot.index);
        }
        Inside { marker: self, index: slot.index }
    }
}

/// The marker slot of one plugin instance; freed together with it.
pub struct GuardSlot {
    marker: Arc<ProcessMarker>,
    index: usize,
}

impl GuardSlot {
    /// The instance enters processing until `Inside` is dropped. No allocations or locks.
    pub fn enter(&self) -> Inside<'_> {
        self.marker.enter(self)
    }
}

impl Drop for GuardSlot {
    fn drop(&mut self) {
        self.marker.taken[self.index].store(false, Ordering::Release);
    }
}

/// While alive, the slot is processing; on drop (including panic unwinding) the mark is removed.
pub struct Inside<'a> {
    marker: &'a ProcessMarker,
    index: usize,
}

impl Drop for Inside<'_> {
    fn drop(&mut self) {
        if let Some(map) = &self.marker.map {
            map.leave(self.index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fakes::Log;

    #[test]
    fn slots_are_reused_after_release_and_running_out_is_logged() {
        let log = Arc::new(Log::default());
        let marker = Arc::new(ProcessMarker::in_memory(log.clone()));
        let slots: Vec<GuardSlot> = (0..SLOTS).map(|n| marker.claim("vst3:a", &format!("A{n}")).unwrap()).collect();
        assert!(marker.claim("vst3:b", "B").is_none());
        assert_eq!(log.recent(10).len(), 1, "running out of slots is logged");
        drop(slots);
        let slot = marker.claim("vst3:b", "B").expect("slots were freed");
        assert_eq!(slot.index, 0);
        drop(slot.enter());
    }

    #[test]
    fn a_marker_without_a_map_hands_out_no_slots() {
        let marker = Arc::new(ProcessMarker::new(None, Arc::new(Log::default())));
        assert!(marker.claim("vst3:a", "A").is_none());
    }
}
