//! A live plugin instance: the component (sound), the controller (parameters) and their life cycle.
//!
//! The entity's job is to create the "component + controller" pair from the bundle factory, take it through
//! initialization, start and stop, and shut it down safely. What to fill buffers with and how to drive blocks is
//! the business of `ProcessRunner` and the effect. Bus layout is `layout`, the parameter list `params` (what counts as real values is `display`),
//! saving and restoring plugin state `state`, which parameters listen to MIDI controllers `midi`.

mod display;
mod layout;
mod midi;
mod params;
mod state;

use std::ffi::{c_char, c_void};
use std::sync::Arc;

use vst3::Steinberg::Vst::{
    BusDirections, BusDirections_, IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentHandler, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IHostApplication, MediaTypes_, ProcessModes_, ProcessSetup,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{FUnknown, IPluginBaseTrait, IPluginFactoryTrait, TUID, kResultOk};
use vst3::{ComPtr, Interface};

use crate::bundle::Bundle;
use crate::guard::GuardSlot;
use crate::host::HostContext;
use crate::midi_map::MidiAssignments;
use crate::scale::ParamScales;
use layout::Layout;
pub use params::{MAX_EXPOSED_PARAMS, ParamDesc, ParamList, ParamShape};

pub type ClassId = [u8; 16];

/// The processing mode the host declares to the plugin (both in `setupProcessing` and in every `process`).
pub const PROCESS_MODE: i32 = ProcessModes_::kRealtime as i32;
/// The host works with 32-bit samples only.
pub const SAMPLE_SIZE: i32 = SymbolicSampleSizes_::kSample32 as i32;

pub struct PluginInstance {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    controller: Option<ComPtr<IEditController>>,
    /// `false` if the controller is the same object as the component (initialized once).
    controller_is_separate: bool,
    /// Which directions of the component ↔ controller connection were made: `Drop` undoes exactly those, so a
    /// half-made connection does not leave the component pointing at a terminated controller.
    component_to_controller: bool,
    controller_to_component: bool,
    component_initialized: bool,
    active: bool,
    processing: bool,
    layout: Layout,
    host: HostContext,
    in_buses: usize,
    out_buses: usize,
    /// Crash marker slot: while the plugin works inside `guarded`, after a crash it is clear the plugin is to blame. Plugin windows have none.
    mark: Option<GuardSlot>,
    bundle: Arc<Bundle>,
}

fn check(result: i32, what: &str) -> Result<(), String> {
    if result == kResultOk { Ok(()) } else { Err(format!("{what}: code {result}")) }
}

fn guid_ptr(guid: &[u8; 16]) -> *const c_char {
    guid.as_ptr().cast()
}

fn create_object<I: Interface>(bundle: &Bundle, class_id: &ClassId) -> Result<ComPtr<I>, String> {
    let mut object: *mut c_void = std::ptr::null_mut();
    // SAFETY: the ids are pointers to 16 bytes, alive for the duration of the call; `object` is a local variable.
    let result = unsafe { bundle.factory().createInstance(guid_ptr(class_id), guid_ptr(&I::IID), &raw mut object) };
    check(result, "createInstance")?;
    // SAFETY: the factory returned a pointer to the requested interface with a counted reference; ownership moves to `ComPtr`.
    unsafe { ComPtr::from_raw(object.cast::<I>()) }.ok_or_else(|| "createInstance returned a null pointer".to_string())
}

impl PluginInstance {
    /// Creates and initializes the "component + controller" pair for class `class_id`.
    pub fn create(bundle: Arc<Bundle>, class_id: &ClassId) -> Result<Self, String> {
        let component = create_object::<IComponent>(&bundle, class_id)?;
        let processor = component.cast::<IAudioProcessor>().ok_or("the component does not support IAudioProcessor")?;
        let mut instance = Self {
            component,
            processor,
            controller: None,
            controller_is_separate: false,
            component_to_controller: false,
            controller_to_component: false,
            component_initialized: false,
            active: false,
            processing: false,
            layout: Layout::Effect,
            host: HostContext::new(),
            in_buses: 0,
            out_buses: 0,
            mark: None,
            bundle,
        };
        instance.initialize()?;
        Ok(instance)
    }

    fn host_context(&self) -> *mut FUnknown {
        self.host.application.as_com_ref::<IHostApplication>().map_or(std::ptr::null_mut(), |app| app.as_ptr().cast())
    }

    fn initialize(&mut self) -> Result<(), String> {
        let context = self.host_context();
        // SAFETY: `context` is our live `IHostApplication` object, which outlives the instance.
        check(unsafe { self.component.initialize(context) }, "component initialize")?;
        self.component_initialized = true;

        self.controller =
            if let Some(same) = self.component.cast::<IEditController>() { Some(same) } else { self.create_separate_controller(context)? };
        self.connect_component_and_controller();
        if let Some(controller) = &self.controller
            && let Some(handler) = self.host.handler.as_com_ref::<IComponentHandler>()
        {
            // SAFETY: the handler is our live object; the controller releases it in `terminate`.
            unsafe { controller.setComponentHandler(handler.as_ptr()) };
        }
        self.in_buses = self.bus_count(BusDirections_::kInput);
        self.out_buses = self.bus_count(BusDirections_::kOutput);
        Ok(())
    }

    fn create_separate_controller(&mut self, context: *mut FUnknown) -> Result<Option<ComPtr<IEditController>>, String> {
        let mut controller_id: TUID = [0; 16];
        // SAFETY: `controller_id` is a local array of the right size.
        if unsafe { self.component.getControllerClassId(&raw mut controller_id) } != kResultOk {
            return Ok(None);
        }
        let class_id: ClassId = controller_id.map(|c| c as u8);
        let controller = create_object::<IEditController>(&self.bundle, &class_id)?;
        // SAFETY: the context is our live `IHostApplication` object.
        check(unsafe { controller.initialize(context) }, "controller initialize")?;
        self.controller_is_separate = true;
        Ok(Some(controller))
    }

    /// Connects the component and the controller if both can exchange messages (`IConnectionPoint`).
    fn connect_component_and_controller(&mut self) {
        if !self.controller_is_separate {
            return;
        }
        let (Some(a), Some(b)) =
            (self.component.cast::<IConnectionPoint>(), self.controller.as_ref().and_then(ComPtr::cast::<IConnectionPoint>))
        else {
            return;
        };
        // SAFETY: both pointers are live interfaces of objects we own.
        self.component_to_controller = unsafe { a.connect(b.as_ptr()) } == kResultOk;
        // SAFETY: same, the connection in the other direction.
        self.controller_to_component = self.component_to_controller && unsafe { b.connect(a.as_ptr()) } == kResultOk;
    }

    fn bus_count(&self, direction: BusDirections) -> usize {
        // SAFETY: a plain component call without pointers.
        let count = unsafe { self.component.getBusCount(MediaTypes_::kAudio as i32, direction as i32) };
        usize::try_from(count).unwrap_or(0)
    }

    pub fn in_buses(&self) -> usize {
        self.in_buses
    }

    pub fn out_buses(&self) -> usize {
        self.out_buses
    }

    /// The plugin's parameter (and window) controller; `None` if the plugin did not provide one.
    pub fn controller(&self) -> Option<&ComPtr<IEditController>> {
        self.controller.as_ref()
    }

    pub fn processor(&self) -> &ComPtr<IAudioProcessor> {
        &self.processor
    }

    /// Mark this instance's work in the crash marker with slot `slot`.
    pub fn watch(&mut self, slot: GuardSlot) {
        self.mark = Some(slot);
    }

    /// `work` on the instance under the "plugin is processing" mark. No allocations or locks: fit for the audio thread.
    pub fn guarded<T>(&mut self, work: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.mark.take();
        let inside = mark.as_ref().map(GuardSlot::enter);
        let result = work(self);
        drop(inside);
        self.mark = mark;
        result
    }

    /// The plugin's parameters for the user and the conversion of their values; without a controller, an empty list.
    pub fn parameters(&self) -> ParamList {
        self.controller
            .as_ref()
            .map_or_else(|| ParamList { params: Vec::new(), scales: ParamScales::default(), total: 0 }, params::read_parameters)
    }

    /// Which plugin parameters listen to MIDI controllers (pitch bend, CC, pressure); without a controller, none.
    pub fn midi_assignments(&self) -> MidiAssignments {
        self.controller.as_ref().map_or_else(MidiAssignments::default, midi::read_midi_assignments)
    }

    /// Turns the plugin on to work at this sample rate. A repeated call stops the previous run first.
    pub fn start(&mut self, sample_rate: f64, max_block: usize) -> Result<(), String> {
        self.stop();
        self.apply_layout()?;
        let mut setup = ProcessSetup {
            processMode: PROCESS_MODE,
            symbolicSampleSize: SAMPLE_SIZE,
            maxSamplesPerBlock: max_block as i32,
            sampleRate: sample_rate,
        };
        // SAFETY: `setup` is a local struct, alive for the duration of the call.
        check(unsafe { self.processor.setupProcessing(&raw mut setup) }, "setupProcessing")?;
        // SAFETY: plain calls without pointers; the setActive -> setProcessing order is set by the spec.
        check(unsafe { self.component.setActive(1) }, "setActive(true)")?;
        self.active = true;
        // SAFETY: see above.
        check(unsafe { self.processor.setProcessing(1) }, "setProcessing(true)")?;
        self.processing = true;
        Ok(())
    }

    /// Stops processing (`setProcessing(false)`, `setActive(false)`); safe to call repeatedly.
    pub fn stop(&mut self) {
        if self.processing {
            // SAFETY: a plain call without pointers.
            unsafe { self.processor.setProcessing(0) };
            self.processing = false;
        }
        if self.active {
            // SAFETY: a plain call without pointers.
            unsafe { self.component.setActive(0) };
            self.active = false;
        }
    }

    /// Resets the plugin's internal state (tails) by restarting processing.
    pub fn reset(&mut self) {
        if self.processing {
            // SAFETY: a plain call without pointers.
            unsafe { self.processor.setProcessing(0) };
            // SAFETY: same.
            unsafe { self.processor.setProcessing(1) };
        }
    }

    pub fn latency_samples(&self) -> u32 {
        // SAFETY: a plain call without pointers.
        unsafe { self.processor.getLatencySamples() }
    }
}

impl Drop for PluginInstance {
    fn drop(&mut self) {
        let mark = self.mark.take();
        let _inside = mark.as_ref().map(GuardSlot::enter);
        self.stop();
        if (self.component_to_controller || self.controller_to_component)
            && let (Some(a), Some(b)) =
                (self.component.cast::<IConnectionPoint>(), self.controller.as_ref().and_then(ComPtr::cast::<IConnectionPoint>))
        {
            if self.component_to_controller {
                // SAFETY: both interfaces are alive, and this direction was connected in `connect_component_and_controller`.
                unsafe { a.disconnect(b.as_ptr()) };
            }
            if self.controller_to_component {
                // SAFETY: same, the connection in the other direction.
                unsafe { b.disconnect(a.as_ptr()) };
            }
        }
        if self.controller_is_separate
            && let Some(controller) = &self.controller
        {
            // SAFETY: the controller was initialized in `create_separate_controller` and is terminated once.
            unsafe { controller.terminate() };
        }
        if self.component_initialized {
            // SAFETY: the component was initialized in `initialize` and is terminated once.
            unsafe { self.component.terminate() };
        }
    }
}
