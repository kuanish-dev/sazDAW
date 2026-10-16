//! The project: the core (model + edit history) and the "department" entities over it, project files and samples.

use std::sync::Arc;

use daw_automation_writer::{LiveAutomationWriter, WriterDeps};
use daw_contracts::{
    AppLog, AssetStore, AutomationHold, DspSink, EffectCatalog, InstrumentCatalog, NoteEffectCatalog, ProcessedAudio, ProjectStorage,
    RecentProjects, SampleDecoder, SampleStore, SnapshotSink, TransportControl,
};
use daw_project::{
    Dependencies, MidiMappingDeps, ProjectAutomation, ProjectCleaner, ProjectClipProcessing, ProjectClips, ProjectFiles, ProjectFolders,
    ProjectGraph, ProjectMarkers, ProjectMidiMappings, ProjectNoteEffects, ProjectNotes, ProjectPluginStates, ProjectSampleCatalog,
    ProjectSampleEditor, ProjectService, ProjectStage, ProjectTakes, ProjectTempo, ProjectTracks,
};
use daw_samples::WavWriter;

/// Where the core gets DSP objects from and where it publishes snapshots.
pub(super) struct Engine {
    pub snapshots: Arc<dyn SnapshotSink>,
    pub dsp: Arc<dyn DspSink>,
    pub transport: Arc<dyn TransportControl>,
    /// Lanes the engine must not play while their params are written live.
    pub hold: Arc<dyn AutomationHold>,
}

/// Project files and sounds.
pub(super) struct Storage {
    pub storage: Arc<dyn ProjectStorage>,
    pub recent: Arc<dyn RecentProjects>,
    pub assets: Arc<dyn AssetStore>,
    pub decoder: Arc<dyn SampleDecoder>,
    pub samples: Arc<dyn SampleStore>,
    /// Clip audio fitted to tempo and pitch (computed outside the audio thread).
    pub processed: Arc<dyn ProcessedAudio>,
}

pub(super) struct ProjectParts {
    pub service: Arc<ProjectService>,
    pub tracks: Arc<ProjectTracks>,
    pub clips: Arc<ProjectClips>,
    pub takes: Arc<ProjectTakes>,
    pub notes: Arc<ProjectNotes>,
    /// Track MIDI effects (the chain before the instrument).
    pub note_effects: Arc<ProjectNoteEffects>,
    pub markers: Arc<ProjectMarkers>,
    pub folders: Arc<ProjectFolders>,
    /// The song tempo map (tempo and meter with their changes).
    pub tempo: Arc<ProjectTempo>,
    pub graph: Arc<ProjectGraph>,
    /// The virtual stage: where each channel stands in the shared room.
    pub stage: Arc<ProjectStage>,
    pub automation: Arc<ProjectAutomation>,
    /// Live automation writing (Touch/Latch): reported to by the interface and by MIDI knobs.
    pub automation_writer: Arc<LiveAutomationWriter>,
    /// Plugin states (presets from their windows): written by plugin windows.
    pub plugin_states: Arc<ProjectPluginStates>,
    pub files: Arc<ProjectFiles>,
    /// Clearing the project content (in one undo step).
    pub clear: Arc<ProjectCleaner>,
    /// The sample catalog ("which sounds are in the project") and the editor ("what we do with one sound") are two jobs.
    pub samples: Arc<ProjectSampleCatalog>,
    pub sample_editor: Arc<ProjectSampleEditor>,
    /// Normalize and reverse of audio clips: decides the clip change, asks the sample editor for the audio work.
    pub clip_processing: Arc<ProjectClipProcessing>,
    /// MIDI controller knobs on params: they move them through the graph and tracks, like the interface.
    pub midi_mappings: Arc<ProjectMidiMappings>,
}

pub(super) fn build(
    engine: Engine,
    disk: Storage,
    instruments: Arc<dyn InstrumentCatalog>,
    effects: Arc<dyn EffectCatalog>,
    note_effects: Arc<dyn NoteEffectCatalog>,
    log: Arc<dyn AppLog>,
) -> ProjectParts {
    let service = Arc::new(ProjectService::new(Dependencies {
        instruments: instruments.clone(),
        effects: effects.clone(),
        note_effects: note_effects.clone(),
        samples: disk.samples.clone(),
        decoder: disk.decoder.clone(),
        transport: engine.transport.clone(),
        snapshots: engine.snapshots,
        dsp: engine.dsp,
        log: log.clone(),
        processed: disk.processed,
    }));
    let samples =
        Arc::new(ProjectSampleCatalog::new(service.clone(), disk.samples.clone(), disk.decoder, disk.assets.clone(), instruments.clone()));
    let sample_editor =
        Arc::new(ProjectSampleEditor::new(service.clone(), samples.clone(), disk.samples.clone(), Arc::new(WavWriter), disk.assets));
    let clip_processing = Arc::new(ProjectClipProcessing::new(service.clone(), disk.samples.clone(), sample_editor.clone()));
    let tracks = Arc::new(ProjectTracks::new(service.clone(), instruments.clone()));
    let graph = Arc::new(ProjectGraph::new(service.clone(), effects.clone()));
    let stage = Arc::new(ProjectStage::new(service.clone()));
    let automation = Arc::new(ProjectAutomation::new(service.clone()));
    let tempo = Arc::new(ProjectTempo::new(service.clone()));
    let automation_writer = Arc::new(LiveAutomationWriter::start(WriterDeps {
        automation: automation.clone(),
        history: service.clone(),
        transport: engine.transport.clone(),
        tempo: tempo.clone(),
        hold: engine.hold,
        log: log.clone(),
    }));
    let midi_mappings = Arc::new(ProjectMidiMappings::new(
        service.clone(),
        MidiMappingDeps {
            graph: graph.clone(),
            tracks: tracks.clone(),
            stage: stage.clone(),
            instruments,
            effects,
            writer: automation_writer.clone(),
            log,
        },
    ));
    ProjectParts {
        tracks,
        clips: Arc::new(ProjectClips::new(service.clone(), disk.samples)),
        takes: Arc::new(ProjectTakes::new(service.clone())),
        notes: Arc::new(ProjectNotes::new(service.clone())),
        note_effects: Arc::new(ProjectNoteEffects::new(service.clone(), note_effects)),
        markers: Arc::new(ProjectMarkers::new(service.clone())),
        folders: Arc::new(ProjectFolders::new(service.clone())),
        tempo,
        graph,
        stage,
        midi_mappings,
        automation,
        automation_writer,
        plugin_states: Arc::new(ProjectPluginStates::new(service.clone())),
        files: Arc::new(ProjectFiles::new(service.clone(), disk.storage, disk.recent)),
        clear: Arc::new(ProjectCleaner::new(service.clone())),
        samples,
        sample_editor,
        clip_processing,
        service,
    }
}
