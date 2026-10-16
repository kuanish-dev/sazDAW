//! Test doubles of the service dependencies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use daw_contracts::codes::{general, graph, notes, plugins, project, samples, tracks};
use daw_contracts::{
    AppLog, AssetStore, AudioBlock, BlockTiming, ContractError, DspCommand, DspSink, Effect, EffectCatalog, EngineSnapshot, ErrorCode,
    Instrument, InstrumentCatalog, KindInfo, L10n, LogEntry, ParamInfo, ParamKind, Params, PluginState, ProcessJob, ProcessedAudio,
    Project, ProjectStorage, RecentProjects, SampleData, SampleDecoder, SampleId, SampleStore, SampleWriter, SnapshotSink, StreamConfig,
    TransportControl,
};

use crate::{
    Dependencies, ProjectAutomation, ProjectClipProcessing, ProjectClips, ProjectFiles, ProjectFolders, ProjectGraph, ProjectMarkers,
    ProjectNoteEffects, ProjectNotes, ProjectPluginStates, ProjectSampleCatalog, ProjectSampleEditor, ProjectService, ProjectStage,
    ProjectTakes, ProjectTempo, ProjectTracks,
};

pub struct NullInstrument;

impl Instrument for NullInstrument {
    fn prepare(&mut self, _: StreamConfig) {}
    fn note_on(&mut self, _: u8, _: f32) {}
    fn note_off(&mut self, _: u8) {}
    fn all_notes_off(&mut self) {}
    fn controller(&mut self, _: daw_contracts::ControllerKind, _: f32) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn render(&mut self, _: &mut AudioBlock) {}
}

pub struct NullEffect;

impl Effect for NullEffect {
    fn prepare(&mut self, _: StreamConfig) {}
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _: &mut [f32], _: Option<&[f32]>) {}
}

fn info(name: &'static str, min: f32, max: f32, default: f32) -> ParamInfo {
    ParamInfo { name, label: L10n::same(name), min, max, default, kind: ParamKind::Number, choices: &[] }
}

fn defaults(kinds: Vec<KindInfo>, kind: &str, unknown: ErrorCode) -> Result<Params, ContractError> {
    kinds
        .into_iter()
        .find(|k| k.kind == kind)
        .map(|k| k.params.iter().map(|p| (p.name.to_string(), p.default)).collect())
        .ok_or_else(|| unknown.with("kind", kind))
}

/// A plugin state with which the catalog doubles refuse to create an object: this way tests see that the state reached
/// the catalog.
pub const BAD_STATE: &[u8] = b"bad";

fn check_state(state: Option<&PluginState>) -> Result<(), ContractError> {
    match state {
        Some(state) if state.component == BAD_STATE => Err(plugins::LOAD_FAILED.with("details", "the plugin state was rejected")),
        _ => Ok(()),
    }
}

/// Instruments "synth" (param gain), "sampler" (recreated when sample changes) and "broken" (never created).
pub struct FakeInstruments;

impl InstrumentCatalog for FakeInstruments {
    fn kinds(&self) -> Vec<KindInfo> {
        vec![
            KindInfo { kind: "synth", label: L10n::same("Synth"), params: vec![info("gain", 0.0, 1.0, 0.5)] },
            KindInfo {
                kind: "sampler",
                label: L10n::same("Sampler"),
                params: vec![ParamInfo { kind: ParamKind::Sample, ..info("sample", 0.0, 1e6, 0.0) }, info("gain", 0.0, 1.0, 0.8)],
            },
            KindInfo { kind: "broken", label: L10n::same("Broken"), params: Vec::new() },
        ]
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        defaults(self.kinds(), kind, tracks::UNKNOWN_INSTRUMENT)
    }

    fn create(&self, kind: &str, _: &Params, state: Option<&PluginState>) -> Result<Box<dyn Instrument>, ContractError> {
        check_state(state)?;
        if kind == "broken" {
            return Err(plugins::LOAD_FAILED.with("details", "broken"));
        }
        self.defaults(kind).map(|_| Box::new(NullInstrument) as Box<dyn Instrument>)
    }

    fn rebuild_on(&self, kind: &str) -> Vec<&'static str> {
        if kind == "sampler" { vec!["sample"] } else { vec![] }
    }
}

pub struct NullNoteEffect;

impl daw_contracts::NoteEffect for NullNoteEffect {
    fn set_param(&mut self, _: &str, _: f32) {}
    fn reset(&mut self) {}
    fn process(&mut self, _: &[daw_contracts::NoteEvent], _: &BlockTiming, _: &mut daw_contracts::NoteBuffer) {}
}

/// MIDI effects "arp" (rate: a choice of four, gate: a number) and "broken" (never created).
pub struct FakeNoteEffects;

const RATES: [L10n; 4] = [L10n::same("1/4"), L10n::same("1/8"), L10n::same("1/16"), L10n::same("1/32")];

impl daw_contracts::NoteEffectCatalog for FakeNoteEffects {
    fn kinds(&self) -> Vec<KindInfo> {
        vec![
            KindInfo {
                kind: "arp",
                label: L10n::same("Arp"),
                params: vec![
                    ParamInfo { kind: ParamKind::Choice, choices: &RATES, ..info("rate", 0.0, 3.0, 2.0) },
                    info("gate", 10.0, 100.0, 50.0),
                ],
            },
            KindInfo { kind: "broken", label: L10n::same("Broken"), params: Vec::new() },
        ]
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        defaults(self.kinds(), kind, notes::UNKNOWN_NOTE_EFFECT)
    }

    fn create(&self, kind: &str, _: &Params) -> Result<Box<dyn daw_contracts::NoteEffect>, ContractError> {
        match kind {
            "arp" => Ok(Box::new(NullNoteEffect)),
            "broken" => Err(general::INTERNAL.with("details", "broken")),
            _ => Err(notes::UNKNOWN_NOTE_EFFECT.with("kind", kind)),
        }
    }
}

pub struct FakeEffects;

impl EffectCatalog for FakeEffects {
    fn kinds(&self) -> Vec<KindInfo> {
        vec![
            KindInfo { kind: "eq", label: L10n::same("EQ"), params: vec![info("gain", -12.0, 12.0, 0.0)] },
            KindInfo {
                kind: "compressor",
                label: L10n::same("Compressor"),
                params: vec![ParamInfo { kind: ParamKind::Switch, ..info("sidechain", 0.0, 1.0, 0.0) }],
            },
        ]
    }

    fn defaults(&self, kind: &str) -> Result<Params, ContractError> {
        defaults(self.kinds(), kind, graph::UNKNOWN_EFFECT)
    }

    fn create(&self, kind: &str, _: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError> {
        check_state(state)?;
        self.defaults(kind).map(|_| Box::new(NullEffect) as Box<dyn Effect>)
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<SampleId, Arc<SampleData>>>);

impl SampleStore for MemoryStore {
    fn get(&self, id: SampleId) -> Option<Arc<SampleData>> {
        self.0.lock().unwrap().get(&id).cloned()
    }
    fn insert(&self, id: SampleId, data: Arc<SampleData>) {
        self.0.lock().unwrap().insert(id, data);
    }
    fn remove(&self, id: SampleId) {
        self.0.lock().unwrap().remove(&id);
    }
}

/// A decoder: files with "missing" in the name cannot be read; the others are one second of mono at 48 kHz.
pub struct FakeDecoder;

impl SampleDecoder for FakeDecoder {
    fn decode(&self, path: &Path) -> Result<SampleData, ContractError> {
        let name = path.to_string_lossy();
        if name.contains("missing") {
            return Err(samples::FILE_READ_FAILED.with("path", name).with("details", "no such file"));
        }
        Ok(SampleData { sample_rate: 48_000, channels: 1, frames: vec![0.1; 48_000] })
    }
}

/// A writer double: remembers what was "written" where.
#[derive(Default)]
pub struct RecordingWriter(pub Mutex<Vec<(PathBuf, usize)>>);

impl SampleWriter for RecordingWriter {
    fn write(&self, path: &Path, data: &SampleData) -> Result<(), ContractError> {
        if path.to_string_lossy().contains("readonly") {
            return Err(samples::WAV_WRITE_FAILED.with("path", path.display()).with("details", "read-only"));
        }
        self.0.lock().unwrap().push((path.to_path_buf(), data.frame_count()));
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryStorage(Mutex<HashMap<PathBuf, Project>>);

impl ProjectStorage for MemoryStorage {
    fn save(&self, path: &Path, project: &Project) -> Result<(), ContractError> {
        self.0.lock().unwrap().insert(path.to_path_buf(), project.clone());
        Ok(())
    }
    fn save_quick(&self, path: &Path, project: &Project) -> Result<(), ContractError> {
        self.save(path, project)
    }
    fn exists(&self, path: &Path) -> bool {
        self.0.lock().unwrap().contains_key(path)
    }
    fn load(&self, path: &Path) -> Result<Project, ContractError> {
        self.0
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| project::FILE_NOT_READ.with("path", path.display()).with("details", "no such project"))
    }
}

/// "Copies" a file to `<project folder>/samples/<name>` without touching the disk.
pub struct FakeAssets;

impl AssetStore for FakeAssets {
    fn adopt(&self, project_file: &Path, source: &Path) -> Result<PathBuf, ContractError> {
        let dir = project_file.parent().unwrap_or(Path::new("")).join("samples");
        if source.starts_with(&dir) {
            return Ok(source.to_path_buf());
        }
        if source.to_string_lossy().contains("unreadable") {
            return Err(project::SAMPLE_NOT_COPIED.with("path", source.display()).with("details", "unreadable"));
        }
        Ok(dir.join(source.file_name().unwrap_or_default()))
    }
    fn new_asset_path(&self, project_file: &Path, name: &str) -> PathBuf {
        project_file.parent().unwrap_or(Path::new("")).join("samples").join(format!("{name}.wav"))
    }
}

/// An in-memory log: tests check that a failure did not vanish silently.
#[derive(Default)]
pub struct MemoryLog(pub Mutex<Vec<LogEntry>>);

impl AppLog for MemoryLog {
    fn record(&self, entry: LogEntry) {
        self.0.lock().unwrap().push(entry);
    }
    fn recent(&self, _: usize) -> Vec<LogEntry> {
        self.0.lock().unwrap().clone()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
}

#[derive(Default)]
pub struct MemoryRecent(pub Mutex<Vec<PathBuf>>);

impl RecentProjects for MemoryRecent {
    fn last(&self) -> Option<PathBuf> {
        self.0.lock().unwrap().first().cloned()
    }
    fn list(&self) -> Vec<PathBuf> {
        self.0.lock().unwrap().clone()
    }
    fn remember(&self, project_file: &Path) {
        let mut list = self.0.lock().unwrap();
        list.retain(|p| p != project_file);
        list.insert(0, project_file.to_path_buf());
    }
    fn forget(&self, project_file: &Path) {
        self.0.lock().unwrap().retain(|p| p != project_file);
    }
}

#[derive(Default)]
pub struct RecordingSnapshots(pub Mutex<Vec<Arc<EngineSnapshot>>>);

impl SnapshotSink for RecordingSnapshots {
    fn publish(&self, snapshot: Arc<EngineSnapshot>) {
        self.0.lock().unwrap().push(snapshot);
    }
}

/// DSP commands as strings for simple comparisons.
#[derive(Default)]
pub struct RecordingDsp(pub Mutex<Vec<String>>);

impl DspSink for RecordingDsp {
    fn send(&self, command: DspCommand) -> Result<(), ContractError> {
        let text = match command {
            DspCommand::AddInstrument { track, .. } => format!("add_instrument:{track}"),
            DspCommand::RemoveInstrument { track } => format!("remove_instrument:{track}"),
            DspCommand::AddEffect { node, .. } => format!("add_effect:{node}"),
            DspCommand::RemoveEffect { node } => format!("remove_effect:{node}"),
            DspCommand::PreviewPlay { .. } => "preview_play".to_string(),
            DspCommand::PreviewStop => "preview_stop".to_string(),
            DspCommand::SetLiveInput { .. } => "set_live_input".to_string(),
            DspCommand::SetNoteEffects { track, chain } => {
                let ids: Vec<String> = chain.iter().map(|(id, _)| id.to_string()).collect();
                format!("note_effects:{track}:[{}]", ids.join(","))
            }
        };
        self.0.lock().unwrap().push(text);
        Ok(())
    }

    fn collect_garbage(&self) {}
}

/// A processed sound double: "processes" instantly (returns a copy of the source sound, one per job, so tests see which
/// computation reached the snapshot: `made`) unless `ready` is off; remembers requests and the last `retain`.
pub struct FakeProcessed {
    samples: Arc<MemoryStore>,
    pub ready: Mutex<bool>,
    pub requests: Mutex<Vec<ProcessJob>>,
    pub kept: Mutex<Vec<ProcessJob>>,
    made: Mutex<HashMap<ProcessJob, Arc<SampleData>>>,
}

impl FakeProcessed {
    /// The sound the double returned for the job (`None` — none).
    pub fn made(&self, job: &ProcessJob) -> Option<Arc<SampleData>> {
        self.made.lock().unwrap().get(job).cloned()
    }
}

impl ProcessedAudio for FakeProcessed {
    fn ready(&self, job: &ProcessJob) -> Option<Arc<SampleData>> {
        self.requests.lock().unwrap().push(job.clone());
        if !*self.ready.lock().unwrap() {
            return None;
        }
        let source = self.samples.get(job.sample)?;
        Some(Arc::clone(self.made.lock().unwrap().entry(job.clone()).or_insert_with(|| Arc::new((*source).clone()))))
    }
    fn compute(&self, job: &ProcessJob) -> Result<Arc<SampleData>, ContractError> {
        self.samples.get(job.sample).ok_or_else(|| samples::NOT_FOUND.with("id", job.sample))
    }
    fn finished(&self) -> u64 {
        0
    }
    fn retain(&self, keep: &[ProcessJob]) {
        *self.kept.lock().unwrap() = keep.to_vec();
    }
}

pub struct Rig {
    pub service: Arc<ProjectService>,
    /// The file life cycle (`ProjectIo`) on top of `service`.
    pub files: ProjectFiles,
    /// The other project entities — each with its own job — on top of the shared core `service`.
    pub tracks: ProjectTracks,
    pub clips: ProjectClips,
    pub takes: ProjectTakes,
    pub notes: ProjectNotes,
    pub markers: ProjectMarkers,
    pub folders: ProjectFolders,
    pub tempo: ProjectTempo,
    pub graph: ProjectGraph,
    pub stage: ProjectStage,
    pub automation: ProjectAutomation,
    pub plugin_states: ProjectPluginStates,
    pub note_effects: ProjectNoteEffects,
    /// The sample catalog and the sample editor — two separate jobs on top of `service`.
    pub catalog: Arc<ProjectSampleCatalog>,
    pub editor: Arc<ProjectSampleEditor>,
    /// Normalize and reverse of audio clips (asks `editor` to measure and write the audio).
    pub processing: ProjectClipProcessing,
    pub transport: Arc<dyn TransportControl>,
    pub snapshots: Arc<RecordingSnapshots>,
    pub dsp: Arc<RecordingDsp>,
    pub storage: Arc<MemoryStorage>,
    pub recent: Arc<MemoryRecent>,
    pub writer: Arc<RecordingWriter>,
    pub log: Arc<MemoryLog>,
    pub samples: Arc<MemoryStore>,
    pub processed: Arc<FakeProcessed>,
}

impl Rig {
    pub fn new() -> Self {
        let (transport, _clock) = daw_transport::create_transport();
        let snapshots = Arc::new(RecordingSnapshots::default());
        let dsp = Arc::new(RecordingDsp::default());
        let storage = Arc::new(MemoryStorage::default());
        let recent = Arc::new(MemoryRecent::default());
        let samples = Arc::new(MemoryStore::default());
        let assets = Arc::new(FakeAssets);
        let writer = Arc::new(RecordingWriter::default());
        let log = Arc::new(MemoryLog::default());
        let processed = Arc::new(FakeProcessed {
            samples: samples.clone(),
            ready: Mutex::new(true),
            requests: Mutex::default(),
            kept: Mutex::default(),
            made: Mutex::default(),
        });
        let service = Arc::new(ProjectService::new(Dependencies {
            instruments: Arc::new(FakeInstruments),
            effects: Arc::new(FakeEffects),
            note_effects: Arc::new(FakeNoteEffects),
            samples: samples.clone(),
            decoder: Arc::new(FakeDecoder),
            transport: transport.clone(),
            snapshots: snapshots.clone(),
            dsp: dsp.clone(),
            log: log.clone(),
            processed: processed.clone(),
        }));
        let tracks = ProjectTracks::new(service.clone(), Arc::new(FakeInstruments));
        let clips = ProjectClips::new(service.clone(), samples.clone());
        let takes = ProjectTakes::new(service.clone());
        let notes = ProjectNotes::new(service.clone());
        let markers = ProjectMarkers::new(service.clone());
        let folders = ProjectFolders::new(service.clone());
        let tempo = ProjectTempo::new(service.clone());
        let graph = ProjectGraph::new(service.clone(), Arc::new(FakeEffects));
        let stage = ProjectStage::new(service.clone());
        let automation = ProjectAutomation::new(service.clone());
        let plugin_states = ProjectPluginStates::new(service.clone());
        let note_effects = ProjectNoteEffects::new(service.clone(), Arc::new(FakeNoteEffects));
        let files = ProjectFiles::new(service.clone(), storage.clone(), recent.clone());
        let catalog = Arc::new(ProjectSampleCatalog::new(
            service.clone(),
            samples.clone(),
            Arc::new(FakeDecoder),
            assets.clone(),
            Arc::new(FakeInstruments),
        ));
        let editor = Arc::new(ProjectSampleEditor::new(service.clone(), catalog.clone(), samples.clone(), writer.clone(), assets));
        let processing = ProjectClipProcessing::new(service.clone(), samples.clone(), editor.clone());
        Self {
            service,
            files,
            tracks,
            clips,
            takes,
            notes,
            markers,
            folders,
            tempo,
            graph,
            stage,
            automation,
            plugin_states,
            note_effects,
            catalog,
            editor,
            processing,
            transport,
            snapshots,
            dsp,
            storage,
            recent,
            writer,
            log,
            samples,
            processed,
        }
    }

    pub fn dsp_log(&self) -> Vec<String> {
        self.dsp.0.lock().unwrap().clone()
    }

    pub fn last_snapshot(&self) -> Arc<EngineSnapshot> {
        self.snapshots.0.lock().unwrap().last().cloned().expect("a snapshot is published")
    }
}
