//! What the plugin sees from outside while its window is open: the edit handler (`IComponentHandler`) and the window frame (`IPlugFrame`).
//!
//! * `EditHandler` turns `beginEdit/performEdit/endEdit` into `ParamBridge` calls (which go to the project), and a parameter
//!   reload (`restartComponent`) and edits of parameters the project does not know into a "state changed" message
//!   for `StateSync` (the window state then goes to the project whole);
//! * `ResizeFrame` remembers the plugin's request to resize the window (`resizeView`): the window is resized by the app's
//!   owner on a timer (`Vst3Editors::sync`), not right from the plugin's call.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use daw_contracts::sync::lock;
use vst3::Class;
use vst3::Steinberg::Vst::RestartFlags_::{kParamValuesChanged, kReloadComponent};
use vst3::Steinberg::Vst::{IComponentHandler, IComponentHandlerTrait, ParamID, ParamValue};
use vst3::Steinberg::{IPlugFrame, IPlugFrameTrait, IPlugView, ViewRect, kInvalidArgument, kResultOk, tresult};

use crate::editor_state::StateSync;
use crate::param_bridge::ParamBridge;

pub struct EditHandler {
    pub bridge: Arc<Mutex<ParamBridge>>,
    pub state: Arc<Mutex<StateSync>>,
}

impl Class for EditHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for EditHandler {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        lock(&self.bridge).begin(id);
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        let mut bridge = lock(&self.bridge);
        bridge.perform(id, value, Instant::now());
        if !bridge.knows(id) {
            // A parameter the project does not know is kept only in the plugin state.
            lock(&self.state).changed(Instant::now());
        }
        kResultOk
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        lock(&self.bridge).end(id, Instant::now());
        kResultOk
    }

    /// A parameter reload (the plugin loaded a preset) is a state change. Latency changes, parameter renames and the like
    /// do not matter to the window: the project does not rebuild the plugin's params.
    unsafe fn restartComponent(&self, flags: i32) -> tresult {
        if flags & (kReloadComponent | kParamValuesChanged) != 0 {
            lock(&self.state).changed(Instant::now());
        }
        kResultOk
    }
}

/// Width and height of the plugin's rectangle; `None` if it is inverted (right edge left of the left one, etc.).
pub fn rect_size(rect: &ViewRect) -> Option<(u32, u32)> {
    let width = u32::try_from(rect.right.checked_sub(rect.left)?).ok()?;
    let height = u32::try_from(rect.bottom.checked_sub(rect.top)?).ok()?;
    Some((width, height))
}

/// The window size the plugin asks for (logical pixels).
pub type SizeRequest = Arc<Mutex<Option<(u32, u32)>>>;

pub struct ResizeFrame {
    pub request: SizeRequest,
}

impl Class for ResizeFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for ResizeFrame {
    unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the plugin passed a pointer to its size struct; we checked it is not null and only read it.
        let rect = unsafe { *new_size };
        let Some(size) = rect_size(&rect) else {
            return kInvalidArgument;
        };
        *lock(&self.request) = Some(size);
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ParamScales;
    use crate::test_fakes::{History, Log, Project};
    use std::collections::HashMap;
    use vst3::ComWrapper;

    /// Tested through the real COM interface, as the plugin sees it.
    #[test]
    #[allow(
        clippy::multiple_unsafe_ops_per_block,
        reason = "the test calls methods of our own COM object in a row: the same safety condition holds for all calls"
    )]
    fn plugin_edits_through_the_com_handler_reach_the_project_in_one_history_group() {
        let (project, history, log) = (Arc::new(Project::default()), Arc::new(History::default()), Arc::new(Log::default()));
        let bridge =
            ParamBridge::new(project.clone(), history.clone(), log, "Test".into(), HashMap::from([(4, 0.5)]), ParamScales::default());
        let state = Arc::new(Mutex::new(StateSync::default()));
        let handler = ComWrapper::new(EditHandler { bridge: Arc::new(Mutex::new(bridge)), state: state.clone() });
        let com = handler.as_com_ref::<IComponentHandler>().expect("handler interface");
        // SAFETY: calls through the COM pointer of our live object, as a plugin would make them.
        unsafe {
            assert_eq!(com.beginEdit(4), kResultOk);
            assert_eq!(com.performEdit(4, 0.75), kResultOk);
            assert_eq!(com.endEdit(4), kResultOk);
        }
        assert_eq!(*project.writes.lock().unwrap(), [("p4".to_string(), 0.75)]);
        assert_eq!(*history.0.lock().unwrap(), ["begin Plugin parameter", "end"]);
        let later = Instant::now() + crate::editor_state::SETTLE;
        assert!(!state.lock().unwrap().due(later), "a knob the project knows is not a state change");
        // SAFETY: same — calls to our live object.
        unsafe {
            assert_eq!(com.performEdit(900, 0.5), kResultOk);
        }
        assert!(state.lock().unwrap().due(later + crate::editor_state::SETTLE), "an unknown parameter lives only in the state");
    }

    #[test]
    fn reloaded_parameters_are_a_state_change() {
        let (project, history, log) = (Arc::new(Project::default()), Arc::new(History::default()), Arc::new(Log::default()));
        let bridge = ParamBridge::new(project, history, log, "Test".into(), HashMap::new(), ParamScales::default());
        let state = Arc::new(Mutex::new(StateSync::default()));
        let handler = ComWrapper::new(EditHandler { bridge: Arc::new(Mutex::new(bridge)), state: state.clone() });
        let com = handler.as_com_ref::<IComponentHandler>().expect("handler interface");
        let later = Instant::now() + crate::editor_state::SETTLE + crate::editor_state::SETTLE;
        // SAFETY: a call through the COM pointer of our live object.
        assert_eq!(unsafe { com.restartComponent(vst3::Steinberg::Vst::RestartFlags_::kLatencyChanged) }, kResultOk);
        assert!(!state.lock().unwrap().due(later), "latency is not state");
        // SAFETY: same.
        assert_eq!(unsafe { com.restartComponent(kParamValuesChanged) }, kResultOk);
        assert!(state.lock().unwrap().due(later));
    }

    #[test]
    fn resize_request_from_the_plugin_is_remembered_for_the_next_tick() {
        let request: SizeRequest = Arc::new(Mutex::new(None));
        let frame = ComWrapper::new(ResizeFrame { request: request.clone() });
        let com = frame.as_com_ref::<IPlugFrame>().expect("frame interface");
        let mut rect = ViewRect { left: 0, top: 0, right: 640, bottom: 480 };
        // SAFETY: our frame does not need the view (null is allowed), `rect` is a local struct.
        assert_eq!(unsafe { com.resizeView(std::ptr::null_mut(), &raw mut rect) }, kResultOk);
        assert_eq!(*request.lock().unwrap(), Some((640, 480)));
        let mut bad = ViewRect { left: 10, top: 0, right: 0, bottom: 5 };
        // SAFETY: same.
        assert_ne!(unsafe { com.resizeView(std::ptr::null_mut(), &raw mut bad) }, kResultOk);
    }
}
