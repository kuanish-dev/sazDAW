//! Output stream: open it on the chosen device with the chosen buffer and show the outcome in the status.

use daw_contracts::codes::audio;
use daw_contracts::{AudioStatus, ContractError};

use super::Host;

impl Host {
    /// Closes the previous output stream and opens a new one. Closing must come BEFORE opening: then the engine lock
    /// is free and two streams do not fight over one device.
    pub(super) fn open_output(&mut self, device: Option<&str>, buffer_frames: Option<u32>) -> Result<(), ContractError> {
        self.output = None;
        let Some(renderer) = self.renderer.clone() else { return Err(audio::OUTPUT_NOT_STARTED.err()) };
        match self.backend.open_output(device, buffer_frames, renderer, self.output_faults.sink()) {
            Ok(open) => {
                self.output = Some(open.stream);
                self.shared.update_status(|status| {
                    *status = AudioStatus {
                        running: true,
                        output_name: Some(open.info.name),
                        sample_rate: open.config.sample_rate,
                        channels: open.config.channels,
                        buffer_range: open.info.buffer_range,
                        error: None,
                    };
                });
                Ok(())
            }
            Err(error) => {
                self.shared.update_status(|status| {
                    status.running = false;
                    status.error = Some(error.clone());
                });
                Err(error)
            }
        }
    }
}
