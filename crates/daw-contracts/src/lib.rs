//! DAW contracts: only types and traits, no implementation.
//!
//! Every layer depends on this crate and on nothing else.
//! Implementations live in their own crates and are wired together in `daw-bootstrap`.

pub mod analysis;
pub mod app;
pub mod audio;
pub mod automation;
pub mod automation_write;
pub mod bounce;
pub mod chord_part;
pub mod clip_processing;
pub mod clips;
pub mod codes;
pub mod effect;
pub mod engine;
pub mod error;
pub mod export;
pub mod folders;
pub mod graph;
pub mod graph_control;
pub mod history;
pub mod i18n;
pub mod instrument;
pub mod log;
pub mod markers;
pub mod master_meter;
pub mod metronome;
pub mod midi;
pub mod model;
pub mod note_effect;
pub mod notes;
pub mod patterns;
pub mod plugin_crash;
pub mod plugin_editor;
pub mod plugin_state;
pub mod preferences;
pub mod presets;
pub mod project_changes;
pub mod project_io;
pub mod record;
pub mod sample;
pub mod stage;
pub mod summary;
pub mod sync;
pub mod takes;
pub mod tempo;
pub mod tempo_map;
pub mod time;
pub mod tracks;
pub mod transport;
pub mod warp;
pub mod workspace;

pub use analysis::{
    DEFAULT_RESOLUTION_BARS, Finding, Level, LevelReport, LevelRequest, LevelTargets, MAX_ANALYSIS_SECONDS, MAX_RESOLUTION_BARS,
    MixAnalysis, MixCheck, SILENCE_DB, Section, Severity, TargetLevels,
};
pub use app::Controls;
pub use audio::{
    AudioBlock, AudioCapture, AudioDeviceInfo, AudioDevices, AudioProcessor, AudioRenderer, AudioSettings, AudioSettingsStore, AudioStatus,
    InputLevel, LiveInput, StreamConfig,
};
pub use automation::{
    AutomationClipEdit, AutomationControl, AutomationShape, CURVE_STRENGTH, ClipDrawing, DrawnClip, MAX_AUTOMATION_CLIPS, MAX_LANES,
    MAX_POINTS, MIN_SHAPE_PERIOD, ShapeRequest, clip_value_at, curve_shape, lane_value_at, value_at,
};
pub use automation_write::{AutomationHold, AutomationWriteMode, AutomationWriter, MAX_HELD_LANES, TOUCH_IDLE_MS, TouchEnd};
pub use bounce::{TrackBounce, TrackRender, TrackSound};
pub use chord_part::{
    Chord, ChordPartControl, ChordPartPreview, ChordPartReport, ChordPartRequest, ChordQuality, ChordTarget, MAX_CHORD_BARS,
    STRUM_STEP_TICKS, STRUM_STEPS, STRUMS, Strum, strum,
};
pub use clip_processing::{ClipProcessing, DEFAULT_NORMALIZE_PEAK_DB, MIN_NORMALIZE_PEAK_DB};
pub use clips::{ClipControl, ClipEdit, FadeShape, MAX_CLIP_GAIN, MAX_CLIP_TICK, MAX_CLIPS, NewNoteClip};
pub use effect::{
    Effect, EffectCatalog, EffectFactory, EffectGroups, InstalledPlugin, MAX_EFFECT_BANDS, PluginCacheStore, PluginScanner, PluginType,
    ScanFailure, ScanReport,
};
pub use engine::{
    BandGains, CAPTURE_LEASE_SECONDS, DspCommand, DspSink, EngineFactory, EngineInstance, EngineLatency, EngineLosses, EngineSnapshot,
    EngineStats, GainReduction, InputMonitor, LiveMessage, LiveNoteSink, MASTER_WINDOW_FRAMES, MAX_WATCHED_NODES, MasterWindow, Meter,
    MonitorStatus, NODE_WINDOW_FRAMES, NodeCursor, NodeScope, NodeWindow, RecordSource, RecordedEvent, SnapshotSink, TrackInput,
};
pub use error::{ContractError, ErrorCode, ErrorKind};
pub use export::{BitDepth, ExportFormat, ExportRange, ExportReport, ExportRequest, Exporter, MAX_TAIL_SECONDS, Normalize, check_tail};
pub use folders::{FolderControl, FolderEdit};
pub use graph::{
    DEFAULT_VOLUME, Edge, Graph, MASTER, MAX_CHANNEL_DELAY_MS, MAX_EDGES, MAX_LEVEL, MAX_NODES, MAX_WIDTH, MonitorMode, Node, NodeId,
    NodeKind, Polarity, Strip, TrackChain,
};
pub use graph_control::GraphControl;
pub use history::{HistoryControl, HistoryMark, HistorySteps};
pub use i18n::{L10n, Language};
pub use instrument::{
    BASS_GUITAR_KIND, ELECTRIC_GUITAR_KIND, GUITAR_KIND, Instrument, InstrumentCatalog, InstrumentFactory, InstrumentGroups, KindInfo,
    ParamInfo, ParamKind, SAMPLER_KIND, SAMPLER_ROOT_PARAM, SAMPLER_SAMPLE_PARAM, SYNTH_KIND, UKULELE_KIND,
};
pub use log::{AppLog, LogEntry, LogLevel};
pub use markers::{MAX_MARKER_NAME, MAX_MARKERS, MarkerControl};
pub use master_meter::{CLIP_DBTP, MASTER_TAP_FRAMES, MasterFrames, MasterMeter, MasterReading, MasterTap, MonoCheck, VECTOR_FRAMES};
pub use metronome::{MetronomeControl, MetronomeSettings, MetronomeSettingsStore};
pub use midi::{MidiFileIo, MidiInputControl, MidiMappingControl};
pub use model::*;
pub use note_effect::{MAX_NOTE_EFFECTS, NOTE_BUFFER_CAPACITY, NoteBuffer, NoteEffect, NoteEffectCatalog, NoteEffectControl, NoteEvent};
pub use notes::{
    GrooveTemplate, MAX_CONTROLLER_POINTS, MAX_GROOVE_SHIFT_TICKS, MAX_HUMANIZE_TICKS, MAX_NOTES_PER_CLIP, NoteControl, NoteTransform,
    Scale, ScaleKey, SwingGrid, Variation,
};
pub use patterns::{
    DrumHit, DrumPartControl, DrumPartPreview, DrumPartReport, DrumPartRequest, DrumPattern, LaneAssignment, LaneHits, LaneTarget,
    PatternLibrary, PatternRole, PatternSlot, STEPS_PER_BAR, VELOCITY_GHOST, VELOCITY_NORMAL, VELOCITY_STRONG,
};
pub use plugin_crash::{PluginCrash, PluginCrashGuard};
pub use plugin_editor::{EditorId, EditorSession, EditorTarget, NativeView, PluginEditors, PluginStateFlush};
pub use plugin_state::{PluginState, PluginStates};
pub use preferences::{
    Accent, Appearance, DEFAULT_AUTOSAVE_SECONDS, MAX_AUTHOR_CHARS, MAX_AUTOSAVE_SECONDS, MAX_UI_SCALE, MIN_AUTOSAVE_SECONDS, MIN_UI_SCALE,
    Preferences, PreferencesStore, Profile, Theme, TrackHeight,
};
pub use presets::{MAX_PRESET_NAME_CHARS, Presets, ProjectTemplates, check_preset_name};
pub use project_changes::{ListChanges, ProjectChanges};
pub use project_io::{ClearOptions, DemoProject, ProjectClear, ProjectIo, ProjectStorage, ProjectView};
pub use record::{AudioRecordControl, AudioRecordStatus, InputChannels, RecordControl, RecordStatus, TrackRecordState};
pub use sample::{
    LibraryEntry, MAX_REGIONS, Region, SampleCatalog, SampleData, SampleDecoder, SampleEditor, SampleInfo, SamplePreview, SampleStore,
    SampleWriter, Waveform,
};
pub use stage::{Listening, MAX_ON_STAGE, Room, StageControl, StagePlace};
pub use summary::{BusSummary, EffectSummary, LaneSummary, ProjectSummary, SectionSummary, SharedEffectSummary, Summary, TrackSummary};
pub use takes::{COMP_CROSSFADE_SECONDS, CompSegment, MAX_COMP_SEGMENTS, TakeControl, TakeSpan};
pub use tempo::TempoControl;
pub use tempo_map::{MAX_SIGNATURE_CHANGES, MAX_TEMPO_CHANGES, SignatureChange, TempoChange, TempoMap};
pub use time::{BlockTiming, Bpm, PPQ, TimeSignature};
pub use tracks::{MAX_TRACK_HEIGHT, MAX_TRACKS, MIN_TRACK_HEIGHT, NewTrack, TRACK_COLORS, TrackControl, check_color};
pub use transport::{RAMP_SEGMENT_FRAMES, TransportClock, TransportControl, TransportState};
pub use warp::{ClipAudio, MAX_PITCH_SEMITONES, MAX_STRETCH, MIN_STRETCH, ProcessJob, ProcessedAudio, StretchAnchor, Warp, WarpEdit};
pub use workspace::{AUDIO_EXTENSIONS, AssetStore, AutosaveControl, ProjectSounds, RecentProjects, SoundSources};
