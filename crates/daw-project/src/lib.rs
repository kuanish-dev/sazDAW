//! The project service. The core (`ProjectService`) is the only owner of the project model and of its edit history
//! (undo/redo). Each job on the project is its own entity on top of the core: `ProjectTracks`, `ProjectClips`,
//! `ProjectNotes`, `ProjectTakes` (takes of an audio clip), `ProjectTempo` (the tempo map), `ProjectKey` (the song's
//! key), `ProjectMarkers` (the song layout), `ProjectFolders` (track folders), `ProjectGraph`, `ProjectAutomation`, `ProjectSampleCatalog`,
//! `ProjectSampleEditor`, `ProjectFiles`, `ProjectPluginStates`, `ProjectBounce` (rendering a track to sound),
//! `ProjectCleaner` (clearing the project content), `ProjectMidiMappings` (MIDI controller knobs on parameters, MIDI
//! learn), `ProjectNoteEffects` (a track's MIDI effects before the instrument), `ProjectClipProcessing` (normalize and
//! reverse of audio clips), `ProjectStage` (the virtual stage: where each channel stands in the shared room).
//!
//! Every edit goes through `ProjectService::edit`: validation, undo history, DSP object sync and publishing the
//! snapshot for the audio thread. Shared lookups and small model edits are pure functions in `ops.rs`; clip rules live
//! in `clip_rules.rs` (tempo fitting and pitch in `warp_rules.rs`), note clip content rules in `note_rules.rs`, track
//! folder rules in `folder_rules.rs`.

mod automation;
mod automation_shapes;
mod bounce;
mod catalog;
mod clear;
mod clip_processing;
mod clip_rules;
mod clips;
mod files;
mod folder_rules;
mod folders;
mod graph;
mod groove;
mod markers;
mod midi_mappings;
mod note_effects;
mod note_rules;
mod note_transforms;
mod notes;
mod ops;
mod plugin_states;
mod sample_editor;
mod service;
mod song_key;
mod stage;
mod take_rules;
mod takes;
mod tempo;
mod tracks;
mod variation;
mod warp_rules;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

pub use automation::ProjectAutomation;
pub use bounce::{BounceParts, ProjectBounce};
pub use catalog::ProjectSampleCatalog;
pub use clear::ProjectCleaner;
pub use clip_processing::ProjectClipProcessing;
pub use clips::ProjectClips;
pub use files::ProjectFiles;
pub use folders::ProjectFolders;
pub use graph::ProjectGraph;
pub use markers::ProjectMarkers;
pub use midi_mappings::{MidiMappingDeps, ProjectMidiMappings};
pub use note_effects::ProjectNoteEffects;
pub use notes::ProjectNotes;
pub use plugin_states::ProjectPluginStates;
pub use sample_editor::ProjectSampleEditor;
pub use service::{Dependencies, ProjectService};
pub use song_key::ProjectKey;
pub use stage::ProjectStage;
pub use takes::ProjectTakes;
pub use tempo::ProjectTempo;
pub use tracks::ProjectTracks;
