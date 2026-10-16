//! Input: open while at least one consumer needs it — the level meter (`watch_input`), recording (`capture_start`) or
//! monitoring; the reading side of the capture queue lives in `Shared`.

use daw_contracts::codes::audio;
use daw_contracts::{ContractError, LogLevel, StreamConfig};

use super::Host;

impl Host {
    /// The input device changed while the input is needed: reopen it. If that fails, watching stops and the failure
    /// is logged; the output is unrelated and stays as chosen.
    pub(super) fn reopen_input_if_watched(&mut self) {
        if self.input.is_none() {
            return;
        }
        self.input = None;
        if let Err(error) = self.open_input() {
            self.watching = false;
            self.monitoring = false;
            self.shared.tap.reset_level();
            self.note(LogLevel::Warn, format!("the input did not open after the device change: {error}"));
        }
    }

    /// Opens the input if it is closed and puts the reading side of the queue into `Shared`.
    fn open_input(&mut self) -> Result<StreamConfig, ContractError> {
        if self.input.is_some() {
            return self.shared.capture_format().ok_or_else(|| audio::INPUT_WITHOUT_QUEUE.err());
        }
        let device = self.shared.settings().input;
        let open = self.backend.open_input(device.as_deref(), self.shared.tap.clone(), self.input_faults.sink())?;
        let format = open.source.format();
        self.shared.tap.reset_level();
        self.shared.tap.live().set_rate(format.sample_rate);
        self.shared.set_source(open.source);
        self.input = Some(open.stream);
        Ok(format)
    }

    /// Closes the input if nobody needs it any more.
    fn close_input_if_unused(&mut self) {
        if !self.watching && !self.capturing && !self.monitoring {
            self.input = None;
            self.shared.tap.reset_level();
            self.shared.tap.live().set_rate(0);
        }
    }

    /// The monitoring input follows the engine's request (`LiveInput::set_wanted`): checked on every host loop step
    /// (at least once per `FAULT_CHECK`), so the input opens within a quarter of a second.
    /// If it fails to open, one log entry per request; the engine shows "input closed" in the monitoring status.
    pub fn follow_monitoring(&mut self) {
        // Without output the engine does not play: its last request is stale, no reason to hold the input.
        let wanted = self.renderer.is_some() && self.shared.tap.live().wanted();
        if !wanted {
            self.monitor_failed = false;
            if self.monitoring {
                self.monitoring = false;
                self.close_input_if_unused();
            }
            return;
        }
        if self.monitoring || self.monitor_failed {
            return;
        }
        match self.open_input() {
            Ok(_) => self.monitoring = true,
            Err(error) => {
                self.monitor_failed = true;
                self.note(LogLevel::Warn, format!("the input for monitoring did not open: {error}"));
            }
        }
    }

    pub fn watch_input(&mut self, on: bool) -> Result<(), ContractError> {
        if !on {
            self.watching = false;
            self.close_input_if_unused();
            return Ok(());
        }
        self.open_input()?;
        self.watching = true;
        Ok(())
    }

    /// Starts capture: opens the input (or reuses the one open for the meter) and switches frame copying on.
    pub fn capture_start(&mut self) -> Result<StreamConfig, ContractError> {
        if self.capturing {
            return Err(audio::CAPTURE_ALREADY_RUNNING.err());
        }
        let format = self.open_input()?;
        // Frames of the previous take must not reach the new one.
        self.shared.clear_capture();
        self.shared.tap.begin_capture();
        self.capturing = true;
        Ok(format)
    }

    pub fn capture_stop(&mut self) {
        self.shared.tap.end_capture();
        self.capturing = false;
        self.close_input_if_unused();
    }
}
