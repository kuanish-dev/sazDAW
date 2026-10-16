//! The app surface: everything the interface controls, as contracts only.
//! Assembled by `daw-bootstrap`; upper layers (RPC, app roots) see only this struct.

use std::sync::Arc;

use crate::{
    AppLog, AudioDevices, AudioRecordControl, AutomationControl, AutomationWriter, AutosaveControl, ChordPartControl, ChordPartPreview,
    ClipControl, ClipProcessing, DemoProject, DrumPartControl, DrumPartPreview, EffectCatalog, EngineStats, Exporter, FolderControl,
    GraphControl, HistoryControl, InstrumentCatalog, LiveNoteSink, MarkerControl, MasterMeter, MetronomeControl, MidiFileIo,
    MidiInputControl, MidiMappingControl, MixAnalysis, MixCheck, MonoCheck, NodeScope, NoteControl, NoteEffectCatalog, NoteEffectControl,
    PatternLibrary, PluginCrashGuard, PluginEditors, PluginScanner, PreferencesStore, Presets, ProjectClear, ProjectIo, ProjectSounds,
    ProjectStorage, ProjectSummary, ProjectTemplates, ProjectView, RecentProjects, RecordControl, SampleCatalog, SampleEditor,
    SamplePreview, SoundSources, StageControl, TakeControl, TempoControl, TrackBounce, TrackControl, TransportControl,
};

/// Everything the interface uses to control the app, as contracts only.
#[derive(Clone)]
pub struct Controls {
    pub transport: Arc<dyn TransportControl>,
    /// Song tempo and time signature (the project's tempo map; edits are undoable).
    pub tempo: Arc<dyn TempoControl>,
    pub metronome: Arc<dyn MetronomeControl>,
    pub tracks: Arc<dyn TrackControl>,
    pub clips: Arc<dyn ClipControl>,
    /// Takes of audio clips (recording over a clip).
    pub takes: Arc<dyn TakeControl>,
    /// Normalize and reverse of audio clips.
    pub clip_processing: Arc<dyn ClipProcessing>,
    pub notes: Arc<dyn NoteControl>,
    /// Track MIDI effects (arpeggiator, chord): the chain before the instrument.
    pub note_effects: Arc<dyn NoteEffectControl>,
    /// Song layout: section markers.
    pub markers: Arc<dyn MarkerControl>,
    pub folders: Arc<dyn FolderControl>,
    pub graph: Arc<dyn GraphControl>,
    /// The virtual stage: where each channel stands in the shared room, and the room.
    pub stage: Arc<dyn StageControl>,
    pub automation: Arc<dyn AutomationControl>,
    /// Writing automation live from param moves (Touch/Latch).
    pub automation_writer: Arc<dyn AutomationWriter>,
    pub history: Arc<dyn HistoryControl>,
    pub project: Arc<dyn ProjectIo>,
    /// The whole project: a copy for `project.get`, export and autosave, and the revision for the interface.
    pub view: Arc<dyn ProjectView>,
    pub demo: Arc<dyn DemoProject>,
    pub samples: Arc<dyn SampleCatalog>,
    pub sample_editor: Arc<dyn SampleEditor>,
    pub preview: Arc<dyn SamplePreview>,
    pub exporter: Arc<dyn Exporter>,
    pub midi_files: Arc<dyn MidiFileIo>,
    pub midi_input: Arc<dyn MidiInputControl>,
    /// MIDI controller knobs on params (MIDI learn).
    pub midi_mappings: Arc<dyn MidiMappingControl>,
    pub live_notes: Arc<dyn LiveNoteSink>,
    pub record: Arc<dyn RecordControl>,
    pub stats: Arc<dyn EngineStats>,
    /// The live sound of one graph node for its editor (the spectrum behind the EQ curve).
    pub node_scope: Arc<dyn NodeScope>,
    /// The live master meters: loudness, true peak and phase.
    pub master_meter: Arc<dyn MasterMeter>,
    /// Listening to the master in mono (a monitoring mode, not saved).
    pub mono: Arc<dyn MonoCheck>,
    pub instruments: Arc<dyn InstrumentCatalog>,
    pub effects: Arc<dyn EffectCatalog>,
    /// MIDI effect kinds and their params.
    pub note_effect_kinds: Arc<dyn NoteEffectCatalog>,
    pub plugins: Arc<dyn PluginScanner>,
    pub plugin_editors: Arc<dyn PluginEditors>,
    /// Whether the previous session crashed in a plugin; app roots call `end_session` on a normal exit.
    pub plugin_crash: Arc<dyn PluginCrashGuard>,
    pub storage: Arc<dyn ProjectStorage>,
    pub recent: Arc<dyn RecentProjects>,
    pub autosave: Arc<dyn AutosaveControl>,
    pub sounds: Arc<dyn ProjectSounds>,
    pub sound_sources: Arc<dyn SoundSources>,
    pub log: Arc<dyn AppLog>,
    pub audio: Arc<dyn AudioDevices>,
    pub audio_record: Arc<dyn AudioRecordControl>,
    pub patterns: Arc<dyn PatternLibrary>,
    pub drum_part: Arc<dyn DrumPartControl>,
    pub drum_preview: Arc<dyn DrumPartPreview>,
    /// A chord progression strummed on one track ("Chord part" above the timeline) and its preview.
    pub chord_part: Arc<dyn ChordPartControl>,
    pub chord_preview: Arc<dyn ChordPartPreview>,
    /// User preferences (the "Settings" page).
    pub preferences: Arc<dyn PreferencesStore>,
    /// Render a track to audio: "Bounce to audio", "Freeze" / "Unfreeze".
    pub bounce: Arc<dyn TrackBounce>,
    /// Project templates ("Save as template…", "New from template").
    pub templates: Arc<dyn ProjectTemplates>,
    pub presets: Arc<dyn Presets>,
    /// Project sound levels (offline render of a snapshot): lets an AI "hear" the result.
    pub analysis: Arc<dyn MixAnalysis>,
    /// Mix check: silent tracks, clipping, suspicious params, missing plugins.
    pub mix_check: Arc<dyn MixCheck>,
    /// Clearing the project contents in one undo step.
    pub clear: Arc<dyn ProjectClear>,
    /// A short overview of the project (mainly for AI: the whole project is too big to read every time).
    pub summary: Arc<dyn ProjectSummary>,
}
