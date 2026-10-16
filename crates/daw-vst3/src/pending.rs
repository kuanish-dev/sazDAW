//! A fixed-capacity "engine to plugin" queue of parameter changes.
//!
//! The engine's `set_param` only writes here (no allocations or calls to the plugin),
//! and at the start of the next `process` the changes go to the plugin through `IParameterChanges`.

use crate::instance::MAX_EXPOSED_PARAMS;
use crate::param_name;
use crate::scale::ParamScales;

/// Queue capacity: how many distinct parameters can change between two blocks. Not less than the number of params the
/// project knows: the initial values of all project params go to the plugin in one batch in the first block.
pub const MAX_PENDING: usize = MAX_EXPOSED_PARAMS;

pub struct PendingChanges {
    items: [(u32, f64); MAX_PENDING],
    len: usize,
}

impl Default for PendingChanges {
    fn default() -> Self {
        Self { items: [(0, 0.0); MAX_PENDING], len: 0 }
    }
}

impl PendingChanges {
    /// Remembers a new parameter value; another change of the same parameter replaces the previous one
    /// (only the block's last value matters to the plugin). Returns `false` if the queue is full.
    pub fn set(&mut self, id: u32, value: f64) -> bool {
        if let Some(existing) = self.items[..self.len].iter_mut().find(|(known, _)| *known == id) {
            existing.1 = value;
            return true;
        }
        if self.len == MAX_PENDING {
            return false;
        }
        self.items[self.len] = (id, value);
        self.len += 1;
        true
    }

    /// A contract parameter value by name `p<id>` in real units: `scales` converts it to normalized
    /// (clamped to the parameter's limits). A foreign name and `NaN` are not changes (that is how the contract says to treat
    /// unknown parameters), they are silently skipped. `false` only on queue overflow: the change
    /// is lost, a failure for `BlockFaults`. No allocations — called from the audio thread.
    pub fn set_named(&mut self, name: &str, value: f32, scales: &ParamScales) -> bool {
        match param_name::parse(name) {
            Some(id) if !value.is_nan() => self.set(id, scales.to_normalized(id, f64::from(value))),
            _ => true,
        }
    }

    pub fn as_slice(&self) -> &[(u32, f64)] {
        &self.items[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ParamScale;

    #[test]
    fn named_values_are_parsed_clamped_and_bad_ones_skipped() {
        let mut queue = PendingChanges::default();
        let scales = ParamScales::default();
        assert!(queue.set_named("p3", 2.0, &scales));
        assert!(queue.set_named("p4", -1.0, &scales));
        assert!(queue.set_named("drive", 0.5, &scales), "a foreign name is not a failure");
        assert!(queue.set_named("p5", f32::NAN, &scales), "NaN is not a change");
        assert_eq!(queue.as_slice(), [(3, 1.0), (4, 0.0)]);
    }

    #[test]
    fn named_values_are_converted_from_plain_units() {
        let mut queue = PendingChanges::default();
        let scales = ParamScales::new(vec![(1, ParamScale::continuous(|n| 100.0 * n)), (2, ParamScale::indexed(3))]);
        assert!(queue.set_named("p1", 25.0, &scales));
        assert!(queue.set_named("p2", 3.0, &scales));
        assert_eq!(queue.as_slice(), [(1, 0.25), (2, 1.0)]);
    }

    #[test]
    fn same_parameter_keeps_last_value() {
        let mut queue = PendingChanges::default();
        assert!(queue.set(5, 0.1));
        assert!(queue.set(7, 0.2));
        assert!(queue.set(5, 0.9));
        assert_eq!(queue.as_slice(), [(5, 0.9), (7, 0.2)]);
    }

    #[test]
    fn full_queue_rejects_new_ids_but_updates_known_ones() {
        let mut queue = PendingChanges::default();
        for id in 0..MAX_PENDING as u32 {
            assert!(queue.set(id, 0.0));
        }
        assert!(!queue.set(1000, 1.0));
        assert!(!queue.set_named("p100000", 0.5, &ParamScales::default()), "the overflow is visible by name too");
        assert!(queue.set(3, 0.5));
        assert_eq!(queue.as_slice()[3], (3, 0.5));
        queue.clear();
        assert!(queue.as_slice().is_empty() && queue.set(1000, 1.0), "after a block the queue is free");
    }
}
