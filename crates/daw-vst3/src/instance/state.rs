//! Instance state: give it (`getState` of the component and the controller) and take it (`setState` of the component, then
//! the controller's `setComponentState` and `setState` — the order from the VST3 spec). Called only on the control
//! side (or on the main thread for a window), never on the audio thread: state streams allocate memory.

use daw_contracts::PluginState;
use vst3::ComWrapper;
use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};
use vst3::Steinberg::{IBStream, kNotImplemented};

use super::{PluginInstance, check};
use crate::stream::MemoryStream;

/// Pointer to the stream's COM interface (lives as long as `stream`).
fn stream_ptr(stream: &ComWrapper<MemoryStream>) -> Result<*mut IBStream, String> {
    stream.as_com_ref::<IBStream>().map(|stream| stream.as_ptr()).ok_or_else(|| "the state stream was not created".to_string())
}

/// "Not implemented" is not a failure: many controllers have no state of their own to keep.
fn optional(result: i32, what: &str) -> Result<(), String> {
    if result == kNotImplemented { Ok(()) } else { check(result, what) }
}

impl PluginInstance {
    /// The plugin's current state. The component must give it; the controller may give nothing.
    pub fn save_state(&self) -> Result<PluginState, String> {
        let component = MemoryStream::empty();
        // SAFETY: the stream is our live COM object and outlives the call; the plugin only writes into it.
        check(unsafe { self.component.getState(stream_ptr(&component)?) }, "component getState")?;
        let mut state = PluginState { component: component.bytes(), controller: Vec::new() };
        if let Some(controller) = &self.controller {
            let stream = MemoryStream::empty();
            // SAFETY: same — our live stream for the duration of the call.
            optional(unsafe { controller.getState(stream_ptr(&stream)?) }, "controller getState")?;
            state.controller = stream.bytes();
        }
        Ok(state)
    }

    /// Applies a saved state: the audio part, then the controller (so its parameters match the sound).
    pub fn load_state(&self, state: &PluginState) -> Result<(), String> {
        let component = MemoryStream::with(state.component.clone());
        // SAFETY: the stream is our live COM object with the state bytes; the plugin only reads from it.
        check(unsafe { self.component.setState(stream_ptr(&component)?) }, "component setState")?;
        let Some(controller) = &self.controller else {
            return Ok(());
        };
        if self.controller_is_separate {
            let mirror = MemoryStream::with(state.component.clone());
            // SAFETY: same — our own stream with the same bytes, positioned at the start.
            optional(unsafe { controller.setComponentState(stream_ptr(&mirror)?) }, "controller setComponentState")?;
        }
        if !state.controller.is_empty() {
            let stream = MemoryStream::with(state.controller.clone());
            // SAFETY: same — our live stream with the controller bytes.
            optional(unsafe { controller.setState(stream_ptr(&stream)?) }, "controller setState")?;
        }
        Ok(())
    }
}
