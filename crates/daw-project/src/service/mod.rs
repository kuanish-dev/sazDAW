//! The project core: owner of the model and of its edit history. Its insides (lock, state, DSP) are hidden in this
//! module; the other entities ask the core through `edit`, `read`, `save_with` and `install` and never reach in.
//! The core itself handles undo and redo (`HistoryControl`, `history.rs`) and reading the whole project
//! (`ProjectView`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use daw_contracts::codes::project;
use daw_contracts::{
    AppLog, ContractError, DspSink, EffectCatalog, InstrumentCatalog, NoteEffectCatalog, ProcessedAudio, Project, ProjectView,
    SampleDecoder, SampleStore, SnapshotSink, TransportControl, sync::lock,
};

mod history;
mod sync;

use history::{Group, History};
use sync::DspState;

/// Everything the service depends on. Contracts only.
pub struct Dependencies {
    pub instruments: Arc<dyn InstrumentCatalog>,
    pub effects: Arc<dyn EffectCatalog>,
    /// Track MIDI effect chains are created from it.
    pub note_effects: Arc<dyn NoteEffectCatalog>,
    pub samples: Arc<dyn SampleStore>,
    pub decoder: Arc<dyn SampleDecoder>,
    pub transport: Arc<dyn TransportControl>,
    pub snapshots: Arc<dyn SnapshotSink>,
    pub dsp: Arc<dyn DspSink>,
    /// Failures to pass DSP objects to the audio thread go here (source `dsp`).
    pub log: Arc<dyn AppLog>,
    /// Processed sound of clips with tempo fitting and pitch shift: it travels to the engine in the snapshot.
    pub processed: Arc<dyn ProcessedAudio>,
}

struct Inner {
    project: Project,
    history: History,
    dsp: DspState,
    version: u64,
    dirty: bool,
    path: Option<PathBuf>,
    warnings: Vec<ContractError>,
    /// Key and time of the last edit, to merge consecutive edits of one parameter.
    last_edit: Option<(String, Instant)>,
    group: Option<Group>,
}

pub struct ProjectService {
    deps: Dependencies,
    inner: Mutex<Inner>,
}

const COALESCE_WINDOW_MS: u128 = 800;

impl ProjectService {
    /// Creates the service with an empty project and publishes it to the engine at once.
    pub fn new(deps: Dependencies) -> Self {
        let service = Self {
            deps,
            inner: Mutex::new(Inner {
                project: Project::empty(),
                history: History::default(),
                dsp: DspState::default(),
                version: 0,
                dirty: false,
                path: None,
                warnings: Vec::new(),
                last_edit: None,
                group: None,
            }),
        };
        service.sync(&mut service.lock());
        service
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// An edit of the model. On error the model stays as it was.
    pub(crate) fn edit<R>(&self, label: &str, f: impl FnOnce(&mut Project) -> Result<R, ContractError>) -> Result<R, ContractError> {
        self.edit_keyed(label, None, f)
    }

    /// The same, but consecutive edits with one `key` (a slider move) merge into one undo step.
    pub(crate) fn edit_keyed<R>(
        &self,
        label: &str,
        key: Option<String>,
        f: impl FnOnce(&mut Project) -> Result<R, ContractError>,
    ) -> Result<R, ContractError> {
        let mut inner = self.lock();
        let before = inner.project.clone();
        let result = match f(&mut inner.project) {
            Ok(result) => result,
            Err(error) => {
                inner.project = before;
                return Err(error);
            }
        };
        if inner.project == before {
            return Ok(result);
        }
        let coalesce = match (&key, &inner.last_edit) {
            (Some(key), Some((last_key, at))) => key == last_key && at.elapsed().as_millis() < COALESCE_WINDOW_MS,
            _ => false,
        };
        // Inside a group no separate undo steps are recorded: the group records one when it closes.
        if !coalesce && inner.group.is_none() {
            inner.history.record(label, before);
        }
        inner.last_edit = key.map(|key| (key, Instant::now()));
        inner.dirty = true;
        self.sync(&mut inner);
        Ok(result)
    }

    /// Publishes the snapshot again if the project has clips with processed sound: a background computation finished
    /// (`ProcessedAudio::finished` grew). The model and history do not change. A tempo change is a regular project
    /// edit and publishes the snapshot itself.
    pub fn refresh_clip_audio(&self) {
        let mut inner = self.lock();
        if Self::needs_processing(&inner) {
            self.publish_snapshot(&mut inner);
        }
    }

    pub(crate) fn read<R>(&self, f: impl FnOnce(&Project) -> R) -> R {
        f(&self.lock().project)
    }

    /// Warnings of the last load or DSP sync (missing files, instruments not created).
    pub(crate) fn take_warnings(&self) -> Vec<ContractError> {
        std::mem::take(&mut self.lock().warnings)
    }

    pub(crate) fn name(&self) -> String {
        self.read(|p| p.name.clone())
    }

    /// The file the project was last loaded from or saved to.
    pub(crate) fn path(&self) -> Option<PathBuf> {
        self.lock().path.clone()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.lock().dirty
    }

    /// An empty untitled project without a file.
    pub(crate) fn reset(&self) {
        self.install(Project::empty(), None, false);
    }

    /// Saves the project through `write` (under the lock, so no edit slips in) and binds it to `path`. "Save as": the
    /// project name follows the file name, but only once the file is written; a failed save changes nothing.
    pub(crate) fn save_with(&self, path: &Path, write: impl FnOnce(&Project) -> Result<(), ContractError>) -> Result<(), ContractError> {
        let mut inner = self.lock();
        let new_name = path.file_stem().filter(|_| inner.path.as_deref() != Some(path)).map(|stem| stem.to_string_lossy().into_owned());
        match new_name {
            Some(name) if name != inner.project.name => {
                let mut renamed = inner.project.clone();
                renamed.name = name;
                write(&renamed)?;
                inner.project = renamed;
                // The name is part of the project everyone reads: a new revision tells them.
                self.publish_snapshot(&mut inner);
            }
            _ => write(&inner.project)?,
        }
        inner.path = Some(path.to_path_buf());
        inner.dirty = false;
        Ok(())
    }

    /// Replaces the current project entirely (load, create, restore).
    pub(crate) fn install(&self, project: Project, path: Option<PathBuf>, dirty: bool) {
        // Decoding is slow: the lock is not held, so the interface does not freeze while a project opens.
        let warnings = self.load_samples(&project);
        let mut inner = self.lock();
        inner.project = project;
        inner.history.clear();
        inner.group = None;
        inner.path = path;
        inner.dirty = dirty;
        inner.last_edit = None;
        inner.warnings = warnings;
        inner.dsp.forget_failures();
        // A new project starts from the beginning and in silence; the old project's cue means nothing here.
        self.deps.transport.set_cue(None);
        self.deps.transport.stop();
        self.sync(&mut inner);
    }

    /// Decodes all project samples into the store; returns warnings about missing files.
    fn load_samples(&self, project: &Project) -> Vec<ContractError> {
        let mut warnings = Vec::new();
        for sample in &project.samples {
            match self.deps.decoder.decode(&sample.path) {
                Ok(data) => self.deps.samples.insert(sample.id, Arc::new(data)),
                Err(error) => warnings
                    .push(project::SAMPLE_NOT_LOADED.with("name", &sample.name).with("path", sample.path.display()).with("details", error)),
            }
        }
        warnings
    }
}

impl ProjectView for ProjectService {
    fn snapshot(&self) -> Project {
        self.lock().project.clone()
    }

    fn revision(&self) -> u64 {
        self.lock().version
    }
}
