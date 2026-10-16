//! Which plugin parameter listens to each MIDI controller: asking the plugin's controller through `IMidiMapping`
//! (event bus 0, channel 0 — this is how the host sends notes).

use vst3::ComPtr;
use vst3::Steinberg::Vst::{IEditController, IMidiMapping, IMidiMappingTrait, ParamID};
use vst3::Steinberg::kResultOk;

use crate::midi_map::MidiAssignments;

/// The plugin's controller table; a plugin without `IMidiMapping` listens to no controllers.
pub fn read_midi_assignments(controller: &ComPtr<IEditController>) -> MidiAssignments {
    let Some(mapping) = controller.cast::<IMidiMapping>() else {
        return MidiAssignments::default();
    };
    MidiAssignments::from_plugin(|number| {
        let mut id: ParamID = 0;
        // SAFETY: `id` is a local variable the plugin writes its answer into; the controller number is within 0..130.
        let result = unsafe { mapping.getMidiControllerAssignment(0, 0, number as i16, &raw mut id) };
        (result == kResultOk).then_some(id)
    })
}
