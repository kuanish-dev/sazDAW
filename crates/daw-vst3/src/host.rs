//! What the plugin sees from outside: the "application" (`IHostApplication`) and the edit handler (`IComponentHandler`).
//!
//! This is the minimum without which some plugins refuse to initialize. The handler of the main instance
//! (the one sounding in the engine) reacts to nothing and always answers "ok": this instance has no interface,
//! and edits from the plugin window come through a separate window instance and its `EditHandler` (`editor_host`).

use std::ffi::c_void;

use vst3::Steinberg::Vst::{
    IComponentHandler, IComponentHandlerTrait, IHostApplication, IHostApplicationTrait, ParamID, ParamValue, String128,
};
use vst3::Steinberg::{TUID, kNotImplemented, kResultOk, tresult};
use vst3::{Class, ComWrapper};

use crate::text::to_utf16;

const HOST_NAME: &str = "DAW";

pub struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kNotImplemented;
        }
        // SAFETY: the plugin passed a pointer to its String128 array; we checked it is not null.
        to_utf16(HOST_NAME, unsafe { &mut *name });
        kResultOk
    }

    unsafe fn createInstance(&self, _cid: *mut TUID, _iid: *mut TUID, obj: *mut *mut c_void) -> tresult {
        // The host creates no messages (`IMessage`) or attribute lists: plugins do without them.
        if !obj.is_null() {
            // SAFETY: a non-null out pointer belongs to the calling plugin and is meant for one value.
            unsafe { *obj = std::ptr::null_mut() };
        }
        kNotImplemented
    }
}

pub struct ComponentHandler;

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: i32) -> tresult {
        kResultOk
    }
}

/// Both objects the host gives to one plugin instance.
pub struct HostContext {
    pub application: ComWrapper<HostApplication>,
    pub handler: ComWrapper<ComponentHandler>,
}

impl HostContext {
    pub fn new() -> Self {
        Self { application: ComWrapper::new(HostApplication), handler: ComWrapper::new(ComponentHandler) }
    }
}
