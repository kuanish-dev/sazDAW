//! Notes for the plugin: the host's `IEventList`.
//!
//! The list is created in advance with spare capacity; `render` only rewrites it (no allocations).

use std::cell::RefCell;

use vst3::Steinberg::Vst::{Event, Event_::EventTypes_, Event__type0, IEventList, IEventListTrait, NoteOffEvent, NoteOnEvent};
use vst3::Steinberg::{kInvalidArgument, kResultFalse, kResultOk, tresult};
use vst3::{Class, ComWrapper};

use crate::notes::{MAX_EVENTS, NoteEvent};

pub struct EventList {
    events: RefCell<Vec<Event>>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

fn to_vst3(event: &NoteEvent) -> Event {
    let (kind, body) = match *event {
        NoteEvent::On { pitch, velocity, id } => (
            EventTypes_::kNoteOnEvent,
            Event__type0 { noteOn: NoteOnEvent { channel: 0, pitch: i16::from(pitch), tuning: 0.0, velocity, length: 0, noteId: id } },
        ),
        NoteEvent::Off { pitch, id } => (
            EventTypes_::kNoteOffEvent,
            Event__type0 { noteOff: NoteOffEvent { channel: 0, pitch: i16::from(pitch), velocity: 0.0, noteId: id, tuning: 0.0 } },
        ),
    };
    Event { busIndex: 0, sampleOffset: 0, ppqPosition: 0.0, flags: 0, r#type: kind as u16, __field0: body }
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> i32 {
        self.events.borrow().len() as i32
    }

    unsafe fn getEvent(&self, index: i32, e: *mut Event) -> tresult {
        if e.is_null() {
            return kInvalidArgument;
        }
        let events = self.events.borrow();
        match usize::try_from(index).ok().and_then(|i| events.get(i)) {
            Some(event) => {
                // SAFETY: the pointer is not null (checked above) and belongs to the plugin for the duration of the call.
                unsafe { *e = *event };
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, _e: *mut Event) -> tresult {
        kResultFalse
    }
}

impl EventList {
    pub fn new() -> ComWrapper<Self> {
        ComWrapper::new(Self { events: RefCell::new(Vec::with_capacity(MAX_EVENTS)) })
    }

    /// Sets the events for one block. The `Vec` capacity is reserved, so no allocations.
    pub fn load(&self, notes: &[NoteEvent]) {
        let mut events = self.events.borrow_mut();
        events.clear();
        events.extend(notes.iter().take(MAX_EVENTS).map(to_vst3));
    }

    pub fn clear(&self) {
        self.events.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(
        clippy::multiple_unsafe_ops_per_block,
        reason = "the test calls methods of our own COM object in a row: the same safety condition holds for all calls"
    )]
    fn plugin_reads_note_events_through_com_interface() {
        let list = EventList::new();
        list.load(&[NoteEvent::On { pitch: 60, velocity: 0.5, id: 7 }, NoteEvent::Off { pitch: 60, id: 7 }]);
        let Some(interface) = list.as_com_ref::<IEventList>() else { unreachable!("the interface is declared in Class") };
        // SAFETY: we call methods of our own object through its COM table; the pointers are local and alive.
        unsafe {
            assert_eq!(interface.getEventCount(), 2);
            let mut event: Event = std::mem::zeroed();
            assert_eq!(interface.getEvent(0, &raw mut event), kResultOk);
            assert_eq!(u32::from(event.r#type), EventTypes_::kNoteOnEvent);
            assert_eq!((event.__field0.noteOn.pitch, event.__field0.noteOn.noteId, event.__field0.noteOn.velocity), (60, 7, 0.5));
            assert_eq!(interface.getEvent(1, &raw mut event), kResultOk);
            assert_eq!(u32::from(event.r#type), EventTypes_::kNoteOffEvent);
            assert_eq!(event.__field0.noteOff.noteId, 7);
            assert_eq!(interface.getEvent(2, &raw mut event), kInvalidArgument);
        }
        list.clear();
        // SAFETY: the same object, see above.
        assert_eq!(unsafe { interface.getEventCount() }, 0);
    }
}
