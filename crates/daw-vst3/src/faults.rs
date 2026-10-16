//! `BlockFaults`: "do not let plugin failures on the audio path vanish silently".
//!
//! Logging is not allowed on the audio thread (writing means allocations and locks), so there only plain counters grow:
//! failed blocks, lost parameter changes, lost notes, controllers the plugin does not listen to. They go to the log on the control side:
//! at the start of the next `prepare` and when the effect or instrument is destroyed (the engine destroys them through the
//! "garbage" queue, not on the audio thread). Also here: starting the instance and logging a refusal, which is a plugin failure in the engine too.

use std::sync::Arc;

use daw_contracts::AppLog;

use crate::instance::PluginInstance;
use crate::journal;
use crate::runner::MAX_FRAMES;

pub struct BlockFaults {
    failed_blocks: u32,
    dropped_params: u32,
    dropped_notes: u32,
    /// Pitch bend, CC and pressure for which the plugin has no parameter (`IMidiMapping`).
    unmapped_controllers: u32,
    plugin: String,
    log: Arc<dyn AppLog>,
}

impl BlockFaults {
    pub fn new(plugin: String, log: Arc<dyn AppLog>) -> Self {
        Self { failed_blocks: 0, dropped_params: 0, dropped_notes: 0, unmapped_controllers: 0, plugin, log }
    }

    /// Audio thread: the block was processed (`true`) or the plugin refused / produced bad numbers (`false`).
    pub fn block(&mut self, ok: bool) {
        if !ok {
            self.failed_blocks = self.failed_blocks.saturating_add(1);
        }
    }

    /// A parameter change was queued (`true`) or lost to an overflow (`false`).
    pub fn param(&mut self, queued: bool) {
        if !queued {
            self.dropped_params = self.dropped_params.saturating_add(1);
        }
    }

    /// A note was queued (`true`) or dropped (`false`).
    pub fn note(&mut self, queued: bool) {
        if !queued {
            self.dropped_notes = self.dropped_notes.saturating_add(1);
        }
    }

    /// A controller was dropped: the plugin does not listen to it.
    pub fn unmapped_controller(&mut self) {
        self.unmapped_controllers = self.unmapped_controllers.saturating_add(1);
    }

    /// Control side: logs what has accumulated (if anything) and resets the counters.
    pub fn report(&mut self) {
        if self.failed_blocks == 0 && self.dropped_params == 0 && self.dropped_notes == 0 && self.unmapped_controllers == 0 {
            return;
        }
        journal::warn(
            &*self.log,
            format!(
                "plugin \"{}\" in the engine: failed blocks {}, lost parameter changes {}, lost notes {}, \
                  MIDI controllers the plugin does not listen to {}",
                self.plugin, self.failed_blocks, self.dropped_params, self.dropped_notes, self.unmapped_controllers
            ),
        );
        (self.failed_blocks, self.dropped_params, self.dropped_notes, self.unmapped_controllers) = (0, 0, 0, 0);
    }

    /// Control side (`prepare`): first the report on the previous run, then starting the instance at `sample_rate`.
    /// `false` when the plugin did not start (the reason is already logged).
    pub fn start(&mut self, instance: &mut PluginInstance, sample_rate: u32) -> bool {
        self.report();
        match instance.start(f64::from(sample_rate), MAX_FRAMES) {
            Ok(()) => true,
            Err(error) => {
                journal::error(&*self.log, format!("plugin \"{}\" did not start: {error}", self.plugin));
                false
            }
        }
    }
}

impl Drop for BlockFaults {
    fn drop(&mut self) {
        self.report();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fakes::Log;
    use daw_contracts::LogLevel;

    #[test]
    fn counters_are_reported_once_and_reset() {
        let log = Arc::new(Log::default());
        let mut faults = BlockFaults::new("Test".into(), log.clone());
        faults.block(true);
        faults.param(true);
        faults.note(true);
        faults.report();
        assert!(log.recent(10).is_empty(), "no failures, empty log");

        faults.block(false);
        faults.block(false);
        faults.param(false);
        faults.note(false);
        faults.report();
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, LogLevel::Warn);
        assert!(entries[0].message.contains("failed blocks 2"), "{}", entries[0].message);
        faults.report();
        assert_eq!(log.recent(10).len(), 1, "counters are reset");
    }

    #[test]
    fn drop_reports_what_is_left() {
        let log = Arc::new(Log::default());
        let mut faults = BlockFaults::new("Test".into(), log.clone());
        faults.unmapped_controller();
        drop(faults);
        assert_eq!(log.recent(10).len(), 1);
    }
}
