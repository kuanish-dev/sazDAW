//! MIDI: import and export of standard files (notes and controllers), live input from devices (notes, controllers and
//! knobs mapped to parameters).

mod file;
mod input;
mod values;

pub use file::MidiFiles;
pub use input::MidiInputService;
