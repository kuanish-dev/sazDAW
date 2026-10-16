//! Parameter changes for the plugin: the host's `IParameterChanges` and `IParamValueQueue`.
//!
//! All queues are created in advance; `process` only rewrites the numbers in the slots (no allocations).
//! Each queue holds one point at frame 0 — "the value from the start of the block".

use std::cell::Cell;

use vst3::Steinberg::Vst::{IParamValueQueue, IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, ParamID, ParamValue};
use vst3::Steinberg::{kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComWrapper};

use crate::pending::{MAX_PENDING, PendingChanges};

struct ValueQueue {
    id: Cell<ParamID>,
    value: Cell<ParamValue>,
}

impl Class for ValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> i32 {
        1
    }

    unsafe fn getPoint(&self, index: i32, sample_offset: *mut i32, value: *mut ParamValue) -> tresult {
        if index != 0 || sample_offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: the pointer is not null (checked above) and belongs to the plugin for the duration of the call.
        unsafe { *sample_offset = 0 };
        // SAFETY: same.
        unsafe { *value = self.value.get() };
        kResultOk
    }

    unsafe fn addPoint(&self, _sample_offset: i32, _value: ParamValue, _index: *mut i32) -> tresult {
        // Input queues are read-only for the plugin.
        kResultFalse
    }
}

/// The queue list of one block: `count` active queues out of those created in advance.
pub struct ParamChanges {
    queues: Vec<ComWrapper<ValueQueue>>,
    count: Cell<usize>,
}

impl Class for ParamChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParamChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.count.get() as i32
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        let Ok(index) = usize::try_from(index) else {
            return std::ptr::null_mut();
        };
        if index >= self.count.get() {
            return std::ptr::null_mut();
        }
        // A pointer without a reference count increment: the host owns the queue, the plugin does not release it.
        self.queues.get(index).and_then(ComWrapper::as_com_ref::<IParamValueQueue>).map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
    }

    unsafe fn addParameterData(&self, _id: *const ParamID, _index: *mut i32) -> *mut IParamValueQueue {
        // The host does not accept the plugin's output changes: `null` is the regular refusal.
        std::ptr::null_mut()
    }
}

impl ParamChanges {
    pub fn new() -> ComWrapper<Self> {
        let queues = (0..MAX_PENDING).map(|_| ComWrapper::new(ValueQueue { id: Cell::new(0), value: Cell::new(0.0) })).collect();
        ComWrapper::new(Self { queues, count: Cell::new(0) })
    }

    /// Sets the accumulated changes in the list (for one block).
    pub fn load(&self, pending: &PendingChanges) {
        for (queue, &(id, value)) in self.queues.iter().zip(pending.as_slice()) {
            queue.id.set(id);
            queue.value.set(value);
        }
        self.count.set(pending.as_slice().len());
    }

    pub fn clear(&self) {
        self.count.set(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::ComRef;

    #[test]
    #[allow(
        clippy::multiple_unsafe_ops_per_block,
        reason = "the test calls methods of our own COM object in a row: the same safety condition holds for all calls"
    )]
    fn plugin_sees_loaded_changes_through_com_interface() {
        let changes = ParamChanges::new();
        let mut pending = PendingChanges::default();
        pending.set(11, 0.25);
        pending.set(12, 0.75);
        changes.load(&pending);

        let Some(interface) = changes.as_com_ref::<IParameterChanges>() else {
            unreachable!("the interface is declared in Class");
        };
        // SAFETY: we call methods of our own object through its COM table; the pointers are local and alive.
        unsafe {
            assert_eq!(interface.getParameterCount(), 2);
            let queue = interface.getParameterData(1);
            let Some(queue) = ComRef::from_raw(queue) else { unreachable!("the queue is not empty") };
            assert_eq!(queue.getParameterId(), 12);
            let (mut offset, mut value) = (-1, -1.0);
            assert_eq!(queue.getPoint(0, &raw mut offset, &raw mut value), kResultOk);
            assert_eq!((offset, value), (0, 0.75));
            assert!(interface.getParameterData(2).is_null());
        }
        changes.clear();
        // SAFETY: the same object, see above.
        assert_eq!(unsafe { interface.getParameterCount() }, 0);
    }
}
