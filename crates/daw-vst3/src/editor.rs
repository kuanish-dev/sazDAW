//! `Editor`: one open plugin interface — "a separate plugin instance, its view and its link to the project".
//!
//! Sound goes through the main instance in the engine; this instance exists only for the window: its controller
//! gets values from the project (`sync`), and edits in the window go to the project through `ParamBridge`. The whole state
//! of the window instance (`capture`/`load`) is carried to the project and back by `Vst3Editors` following `StateSync`.
//! Everything here is called from the app's main thread: plugin interfaces require it.

use std::ffi::c_void;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use daw_contracts::codes::plugins;
use daw_contracts::sync::lock;
use daw_contracts::{ContractError, PluginState};
use vst3::Steinberg::Vst::{IComponentHandler, IEditController, IEditControllerTrait};
use vst3::Steinberg::{IPlugFrame, IPlugView, IPlugViewTrait, ViewRect, kPlatformTypeNSView, kResultOk, kResultTrue};
use vst3::{ComPtr, ComWrapper};

use crate::editor_host::{EditHandler, ResizeFrame, SizeRequest, rect_size};
use crate::editor_state::StateSync;
use crate::instance::PluginInstance;
use crate::param_bridge::ParamBridge;

/// The "editor window" view name in `IEditController::createView`.
const EDITOR_VIEW: &[u8] = b"editor\0";

/// Why the window did not open: "the plugin has no window" is normal, everything else is a failure.
pub enum OpenError {
    NoWindow,
    Failed(String),
}

pub struct Editor {
    // Field order = drop order: the view first, then the plugin instance, then our objects that it holds.
    view: ComPtr<IPlugView>,
    attached: bool,
    instance: PluginInstance,
    _frame: ComWrapper<ResizeFrame>,
    _handler: ComWrapper<EditHandler>,
    size_request: SizeRequest,
    bridge: Arc<Mutex<ParamBridge>>,
    state: Arc<Mutex<StateSync>>,
    /// The state the window was created with (before the project state was loaded): the window returns to it when an undo
    /// removes the state from the project.
    pub initial: Option<PluginState>,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
}

// SAFETY: the COM pointers inside are not thread-safe by themselves, but `Editor` is called only from the app's main
// thread (the `PluginEditors` contract); `Vst3Editors` puts it into a `Mutex` to own the set of windows.
unsafe impl Send for Editor {}

fn view_size(view: &ComPtr<IPlugView>) -> Result<(u32, u32), String> {
    let mut rect = ViewRect { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `rect` is a local struct the plugin writes the size into.
    if unsafe { view.getSize(&raw mut rect) } != kResultOk {
        return Err("getSize failed".into());
    }
    match rect_size(&rect) {
        Some((width, height)) if width > 0 && height > 0 => Ok((width, height)),
        _ => Err(format!("the plugin reported an odd window size {}x{}", rect.right - rect.left, rect.bottom - rect.top)),
    }
}

/// A plugin view with our frame, not yet attached to a window.
struct FramedView {
    view: ComPtr<IPlugView>,
    frame: ComWrapper<ResizeFrame>,
    size_request: SizeRequest,
    width: u32,
    height: u32,
    resizable: bool,
}

impl FramedView {
    fn create(controller: &ComPtr<IEditController>) -> Result<Self, OpenError> {
        // SAFETY: the name is a nul-terminated C string; the plugin returns a view with a counted reference or a null pointer.
        let raw = unsafe { controller.createView(EDITOR_VIEW.as_ptr().cast()) };
        // SAFETY: ownership of the reference moves to `ComPtr`; a null pointer gives `None`.
        let view = unsafe { ComPtr::from_raw(raw) }.ok_or(OpenError::NoWindow)?;
        // SAFETY: the platform name is a static C string.
        if unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) } != kResultOk {
            return Err(OpenError::Failed("the plugin cannot show its window in an NSView".into()));
        }
        let size_request: SizeRequest = Arc::new(Mutex::new(None));
        let frame = ComWrapper::new(ResizeFrame { request: size_request.clone() });
        let frame_ref = frame.as_com_ref::<IPlugFrame>().ok_or_else(|| OpenError::Failed("the window frame was not created".into()))?;
        // SAFETY: the frame is our live object; before it is released the plugin gets a null frame (here on error, otherwise in the window's `Drop`).
        unsafe { view.setFrame(frame_ref.as_ptr()) };
        let size = view_size(&view);
        let (width, height) = match size {
            Ok(size) => size,
            Err(error) => {
                // SAFETY: the plugin stops referring to the frame that is about to be released.
                unsafe { view.setFrame(std::ptr::null_mut()) };
                return Err(OpenError::Failed(error));
            }
        };
        // SAFETY: a plain call without pointers.
        let resizable = unsafe { view.canResize() } == kResultTrue;
        Ok(Self { view, frame, size_request, width, height, resizable })
    }
}

impl Editor {
    /// Creates the plugin view (without showing it): the controller gets our frame and edit handler.
    pub fn create(instance: PluginInstance, bridge: Arc<Mutex<ParamBridge>>) -> Result<Self, OpenError> {
        let controller = instance.controller().ok_or_else(|| OpenError::Failed("the plugin has no controller".into()))?.clone();
        let state = Arc::new(Mutex::new(StateSync::default()));
        let initial = instance.save_state().ok();
        let handler = ComWrapper::new(EditHandler { bridge: bridge.clone(), state: state.clone() });
        let handler_ref =
            handler.as_com_ref::<IComponentHandler>().ok_or_else(|| OpenError::Failed("the edit handler was not created".into()))?;
        // SAFETY: the handler is our live object and outlives the instance (the `_handler` field is dropped after `instance`);
        // if the window is not created, the handler is removed below before it is released.
        unsafe { controller.setComponentHandler(handler_ref.as_ptr()) };
        match FramedView::create(&controller) {
            Ok(framed) => Ok(Self {
                view: framed.view,
                attached: false,
                instance,
                _frame: framed.frame,
                _handler: handler,
                size_request: framed.size_request,
                bridge,
                state,
                initial,
                width: framed.width,
                height: framed.height,
                resizable: framed.resizable,
            }),
            Err(error) => {
                // SAFETY: the controller forgets our handler before it is released together with this function.
                unsafe { controller.setComponentHandler(std::ptr::null_mut()) };
                Err(error)
            }
        }
    }

    /// Attaches the plugin view to the parent `NSView`.
    pub fn attach(&mut self, parent: usize) -> Result<(), ContractError> {
        if self.attached {
            return Err(plugins::ALREADY_ATTACHED.err());
        }
        if parent == 0 {
            return Err(plugins::NO_PARENT_VIEW.err());
        }
        // SAFETY: `parent` is a pointer to the window's live `NSView`, passed by the window owner (the `attach` contract);
        // the call comes from the main thread, as the plugin requires.
        let result = unsafe { self.view.attached(parent as *mut c_void, kPlatformTypeNSView) };
        if result != kResultOk {
            return Err(plugins::ATTACH_FAILED.with("code", result));
        }
        self.attached = true;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), ContractError> {
        let (Ok(right), Ok(bottom)) = (i32::try_from(width), i32::try_from(height)) else {
            return Err(plugins::WINDOW_TOO_LARGE.with("width", width).with("height", height));
        };
        let mut rect = ViewRect { left: 0, top: 0, right, bottom };
        // SAFETY: `rect` is a local struct, the plugin only reads it.
        unsafe { self.view.onSize(&raw mut rect) };
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Tick: flushes held-back edits, pulls project values into the window's controller and returns the plugin's
    /// resize request. An error means the target is gone from the project.
    pub fn sync(&mut self, now: Instant) -> Result<Option<(u32, u32)>, ContractError> {
        let changes = {
            let mut bridge = lock(&self.bridge);
            bridge.flush(now);
            bridge.pull()?
        };
        // The lock is released: the controller might call the handler meanwhile, and it takes the same lock.
        if let Some(controller) = self.instance.controller() {
            for (id, value) in changes {
                // SAFETY: a plain controller call without pointers.
                unsafe { controller.setParamNormalized(id, value) };
            }
        }
        Ok(lock(&self.size_request).take())
    }

    pub fn capture(&self) -> Result<PluginState, String> {
        self.instance.save_state()
    }

    /// Loads the whole state into the window; the window gets the project's parameter values again on the next `sync`.
    pub fn load(&mut self, state: &PluginState) -> Result<(), String> {
        self.instance.load_state(state)?;
        lock(&self.bridge).forget_shown();
        Ok(())
    }

    /// Rules for writing the window state (the plugin's edit handler gets them too).
    pub fn state(&self) -> MutexGuard<'_, StateSync> {
        lock(&self.state)
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        lock(&self.bridge).finish(Instant::now());
        if self.attached {
            // SAFETY: the view was attached in `attach` and is removed once.
            unsafe { self.view.removed() };
        }
        // SAFETY: the plugin stops referring to our frame before it is released.
        unsafe { self.view.setFrame(std::ptr::null_mut()) };
    }
}
