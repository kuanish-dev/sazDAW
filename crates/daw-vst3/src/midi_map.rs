//! `MidiAssignments`: "which plugin parameter handles each MIDI controller". VST3 does not accept CC and pitch bend
//! as events: the plugin itself says (`IMidiMapping`) which of its parameters listens to a controller, and the host changes that parameter.
//! The table is built on the control side when the instrument is created (`instance::read_midi_assignments`); the audio
//! thread only reads it — without allocations.

use daw_contracts::{ControllerKind, MAX_CC_NUMBER};

/// VST3 controller numbers (`ControllerNumbers`): 0..=127 are CC, then channel pressure and pitch bend.
pub const VST_AFTERTOUCH: usize = 128;
pub const VST_PITCH_BEND: usize = 129;
/// How many numbers to ask the plugin about.
pub const VST_CONTROLLERS: usize = 130;

#[derive(Clone)]
pub struct MidiAssignments {
    params: [Option<u32>; VST_CONTROLLERS],
}

impl Default for MidiAssignments {
    fn default() -> Self {
        Self { params: [None; VST_CONTROLLERS] }
    }
}

impl MidiAssignments {
    /// The table from the plugin's answers: `ask(number)` is its parameter for a VST3 controller number, or `None`.
    pub fn from_plugin(mut ask: impl FnMut(usize) -> Option<u32>) -> Self {
        let mut assignments = Self::default();
        for (number, slot) in assignments.params.iter_mut().enumerate() {
            *slot = ask(number);
        }
        assignments
    }

    /// The plugin parameter for a controller and the normalized value (`0..=1`; pitch bend `-1..=1` -> `0..=1`);
    /// `None` when the plugin does not listen to this controller.
    pub fn change_for(&self, kind: ControllerKind, value: f32) -> Option<(u32, f64)> {
        let (number, normalized) = match kind {
            ControllerKind::Cc { number } if number <= MAX_CC_NUMBER => (usize::from(number), f64::from(value)),
            ControllerKind::Cc { .. } => return None,
            ControllerKind::Aftertouch => (VST_AFTERTOUCH, f64::from(value)),
            ControllerKind::PitchBend => (VST_PITCH_BEND, f64::midpoint(f64::from(value), 1.0)),
        };
        self.params[number].map(|id| (id, normalized.clamp(0.0, 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controllers_map_to_the_plugin_parameters_it_reported() {
        let map = MidiAssignments::from_plugin(|number| match number {
            1 => Some(10),
            VST_AFTERTOUCH => Some(11),
            VST_PITCH_BEND => Some(12),
            _ => None,
        });
        assert_eq!(map.change_for(ControllerKind::Cc { number: 1 }, 0.25), Some((10, 0.25)));
        assert_eq!(map.change_for(ControllerKind::Aftertouch, 1.0), Some((11, 1.0)));
        assert_eq!(map.change_for(ControllerKind::PitchBend, -1.0), Some((12, 0.0)));
        assert_eq!(map.change_for(ControllerKind::PitchBend, 0.0), Some((12, 0.5)));
        assert_eq!(map.change_for(ControllerKind::Cc { number: 64 }, 1.0), None, "the plugin does not listen to the pedal");
        assert_eq!(map.change_for(ControllerKind::Cc { number: 125 }, 1.0), None);
    }
}
