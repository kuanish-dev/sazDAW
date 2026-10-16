//! Thin boundary with system audio: everything real cpal does, in five methods.
//! It lets the host thread (`host.rs`) be tested with a fake device and no sound card.
//! Private to the crate: only `CpalAudio` is visible outside.

use std::any::Any;
use std::sync::{Arc, Mutex};

use daw_contracts::codes::audio;
use daw_contracts::{AudioDeviceInfo, AudioRenderer, ContractError, StreamConfig};

use crate::capture::{CaptureSource, InputTap};
use crate::faults::FaultSink;

/// The engine shared between output streams: it cannot be given away to a closing stream, it must survive a
/// device change.
pub(crate) type SharedRenderer = Arc<Mutex<Box<dyn AudioRenderer>>>;

/// Body of the output callback. Real time: waiting for the lock is not allowed, hence `try_lock`.
/// The lock is taken only while streams are swapped, and then this block is silent. No allocations.
pub(crate) fn render_block(renderer: &SharedRenderer, data: &mut [f32]) {
    match renderer.try_lock() {
        Ok(mut renderer) => renderer.render(data),
        Err(_) => data.fill(0.0),
    }
}

/// What is known about the chosen output device.
pub(crate) struct OutputInfo {
    pub name: String,
    pub buffer_range: Option<(u32, u32)>,
}

pub(crate) struct OpenOutput {
    /// Sound plays while this lives; a cpal stream cannot move between threads, so it is hidden behind `Any`.
    pub stream: Box<dyn Any>,
    pub config: StreamConfig,
    pub info: OutputInfo,
}

pub(crate) struct OpenInput {
    /// The input is open while this lives.
    pub stream: Box<dyn Any>,
    pub source: CaptureSource,
}

pub(crate) trait Backend {
    fn outputs(&self) -> Vec<AudioDeviceInfo>;
    fn inputs(&self) -> Vec<AudioDeviceInfo>;
    /// The device and its allowed buffer, without opening anything: the choice is checked while the old stream still plays.
    fn probe_output(&self, device: Option<&str>) -> Result<OutputInfo, ContractError>;
    /// Opens the stream, calls `renderer.prepare` and only then starts playback.
    fn open_output(
        &self,
        device: Option<&str>,
        buffer_frames: Option<u32>,
        renderer: SharedRenderer,
        faults: FaultSink,
    ) -> Result<OpenOutput, ContractError>;
    /// Input stream: writes peaks and (while recording) frames to the queue that `CaptureSource` reads.
    fn open_input(&self, device: Option<&str>, tap: InputTap, faults: FaultSink) -> Result<OpenInput, ContractError>;
}

/// A host without devices: for tests and scripts that need no sound and must not depend on a sound card.
pub(crate) struct NoDevices;

impl Backend for NoDevices {
    fn outputs(&self) -> Vec<AudioDeviceInfo> {
        Vec::new()
    }

    fn inputs(&self) -> Vec<AudioDeviceInfo> {
        Vec::new()
    }

    fn probe_output(&self, device: Option<&str>) -> Result<OutputInfo, ContractError> {
        match device {
            Some(name) => Err(audio::OUTPUT_DEVICE_NOT_FOUND.with("name", name)),
            None => Err(audio::NO_OUTPUT_DEVICE.err()),
        }
    }

    fn open_output(
        &self,
        device: Option<&str>,
        _buffer_frames: Option<u32>,
        _renderer: SharedRenderer,
        _faults: FaultSink,
    ) -> Result<OpenOutput, ContractError> {
        Err(self.probe_output(device).err().unwrap_or_else(|| audio::NO_OUTPUT_DEVICE.err()))
    }

    fn open_input(&self, device: Option<&str>, _tap: InputTap, _faults: FaultSink) -> Result<OpenInput, ContractError> {
        Err(match device {
            Some(name) => audio::INPUT_DEVICE_NOT_FOUND.with("name", name),
            None => audio::NO_INPUT_DEVICE.err(),
        })
    }
}
