//! MIDI: files (`MidiFileIo`), live input from devices (`MidiInputControl`) and controller knobs on params
//! (`MidiMappingControl`, MIDI learn).

use std::path::Path;

use crate::{ClipId, ContractError, MidiMapping, ParamTarget, TrackId};

/// Import and export of standard MIDI files.
pub trait MidiFileIo: Send + Sync {
    /// Writes the note clips of all project tracks (one file track per project track) with their controller lanes
    /// (pitch bend, CC, aftertouch) and the tempo map (smooth ramps become staircases of steps).
    fn export_project(&self, path: &Path) -> Result<(), ContractError>;
    /// Writes the notes and controller lanes of one clip.
    fn export_clip(&self, clip: ClipId, path: &Path) -> Result<(), ContractError>;
    /// Replaces the clip's notes and controller lanes with the notes and controllers from the file (the first file
    /// track with notes); returns the number of notes.
    fn import_into(&self, clip: ClipId, path: &Path) -> Result<usize, ContractError>;
    /// Replaces the project's tempo map with tempo and time signatures from the file (events of all tracks; no event at
    /// tick 0 means 120 BPM and 4/4, per the MIDI standard). Smooth ramps in MIDI are steps and stay steps.
    fn import_tempo(&self, path: &Path) -> Result<(), ContractError>;
}

/// External MIDI input devices.
pub trait MidiInputControl: Send + Sync {
    /// Input device names; if MIDI is unavailable, an empty list (the failure is logged).
    fn devices(&self) -> Vec<String>;
    /// Connects a device; notes and controllers (pitch bend, CC, channel pressure) go to track `target`, except CCs
    /// taken by `MidiMappingControl::control_change` (learning or mapped to a param).
    fn connect(&self, device: &str, target: TrackId) -> Result<(), ContractError>;
    /// The track that receives the connected device's notes. Not checked: a note to a missing or audio track simply
    /// does not sound. The next `connect` sets the track anew.
    fn set_target(&self, target: TrackId);
    fn disconnect(&self);
    fn connected_device(&self) -> Option<String>;
}

/// MIDI controller knobs on project params (MIDI learn). Job: know which knob moves which param, and move it when the
/// knob turns. Targets are the same as for automation (`ParamTarget`) and are checked the same way.
/// A mapping to a target that is gone (the node was deleted) has no effect and is not shown; undoing the deletion
/// brings it back.
pub trait MidiMappingControl: Send + Sync {
    /// Project mappings to existing targets.
    fn mappings(&self) -> Vec<MidiMapping>;
    /// Arms learning: the next incoming CC is mapped to `target` (replacing this target's previous mapping) in one undo
    /// step. A missing target is `ErrorKind::NotFound`/`ErrorKind::Invalid`, as in `AutomationControl::add_lane`. A
    /// repeated call changes the target.
    fn learn(&self, target: ParamTarget) -> Result<(), ContractError>;
    fn cancel_learn(&self);
    /// Which target is being learned; `None` means learning is not armed.
    fn learning(&self) -> Option<ParamTarget>;
    /// Removes the target's mapping; a missing one is `ErrorKind::Invalid`. One undo step.
    fn remove_mapping(&self, target: &ParamTarget) -> Result<(), ContractError>;
    /// An incoming CC from a device: `channel` 0..=15, `cc` 0..=127, `value` 0..=127. If learning is armed, creates a
    /// mapping; if this CC has mappings, sets their params (like moving a knob in the interface: an undo step merged
    /// with its neighbours) and hands each move to `AutomationWriter::touch` first, so it is written to the lane while
    /// automation writing is on and the song plays. `true` means the CC was taken and does not reach the track's
    /// instrument. An edit failure is logged.
    fn control_change(&self, channel: u8, cc: u8, value: u8) -> bool;
}
