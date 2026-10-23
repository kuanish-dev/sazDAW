//! `TemplateFolder` (`ProjectTemplates`): "remember project layouts and start new projects from them".
//!
//! A template is a small project folder in the app state folder: `templates/<name>/template.dawproj` and `samples/`
//! (sampler sounds). It is written and read in the same project format (`ProjectStorage`), so copying sounds and their
//! paths work as in a regular project.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::{project as project_codes, settings};
use daw_contracts::{
    ContractError, InstrumentCatalog, ParamKind, Project, ProjectIo, ProjectStorage, ProjectTemplates, ProjectView, SampleId,
    check_preset_name,
};

/// Name of the project file inside a template folder.
const TEMPLATE_FILE: &str = "template.dawproj";

pub struct TemplateFolder {
    dir: PathBuf,
    view: Arc<dyn ProjectView>,
    storage: Arc<dyn ProjectStorage>,
    files: Arc<dyn ProjectIo>,
    instruments: Arc<dyn InstrumentCatalog>,
}

impl TemplateFolder {
    /// Templates live in `<state_dir>/templates`; the folder is created on the first save.
    pub fn new(
        state_dir: &Path,
        view: Arc<dyn ProjectView>,
        storage: Arc<dyn ProjectStorage>,
        files: Arc<dyn ProjectIo>,
        instruments: Arc<dyn InstrumentCatalog>,
    ) -> Self {
        Self { dir: state_dir.join("templates"), view, storage, files, instruments }
    }

    /// The template's own folder: the checked name is one path component, never the templates folder itself.
    fn folder(&self, name: &str) -> Result<PathBuf, ContractError> {
        Ok(self.dir.join(check_preset_name(name)?))
    }

    fn file(&self, name: &str) -> Result<PathBuf, ContractError> {
        Ok(self.folder(name)?.join(TEMPLATE_FILE))
    }

    fn existing(&self, name: &str) -> Result<PathBuf, ContractError> {
        let file = self.file(name)?;
        if self.storage.exists(&file) { Ok(file) } else { Err(settings::TEMPLATE_NOT_FOUND.with("name", name.trim())) }
    }

    /// Sample-reference parameters of every instrument kind (from the catalog, not by kind names).
    fn sample_params(&self) -> HashMap<&'static str, Vec<&'static str>> {
        self.instruments
            .kinds()
            .into_iter()
            .map(|kind| (kind.kind, kind.params.iter().filter(|p| p.kind == ParamKind::Sample).map(|p| p.name).collect()))
            .collect()
    }
}

/// The project's layout: no clips, automation, loop, markers or samples except instrument sounds
/// (`sample_params` are the sample-reference parameters per kind); freezing is removed.
fn structure_of(project: &Project, sample_params: &HashMap<&str, Vec<&str>>) -> Project {
    let mut structure = project.clone();
    structure.clips.clear();
    structure.automation.clear();
    structure.loop_region = None;
    structure.markers.clear();
    let mut used = BTreeSet::<SampleId>::new();
    for spec in structure.tracks.iter_mut().filter_map(|t| t.instrument.as_mut()) {
        spec.frozen = None;
        for name in sample_params.get(spec.kind.as_str()).into_iter().flatten() {
            if let Some(value) = spec.params.get(*name) {
                used.insert(*value as SampleId);
            }
        }
    }
    structure.samples.retain(|sample| used.contains(&sample.id));
    structure
}

impl ProjectTemplates for TemplateFolder {
    fn list(&self) -> Result<Vec<String>, ContractError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(settings::TEMPLATES_NOT_READ.with("path", self.dir.display()).with("details", error)),
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().join(TEMPLATE_FILE).is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    }

    fn save(&self, name: &str) -> Result<(), ContractError> {
        let folder = self.folder(name)?;
        let structure = structure_of(&self.view.snapshot(), &self.sample_params());
        if folder.exists() {
            fs::remove_dir_all(&folder).map_err(|e| settings::TEMPLATE_NOT_REPLACED.with("name", name.trim()).with("details", e))?;
        }
        self.storage.save(&folder.join(TEMPLATE_FILE), &structure)
    }

    fn remove(&self, name: &str) -> Result<(), ContractError> {
        self.existing(name)?;
        fs::remove_dir_all(self.folder(name)?).map_err(|e| settings::TEMPLATE_NOT_DELETED.with("name", name.trim()).with("details", e))
    }

    fn create_project(&self, template: &str, project_file: &Path) -> Result<(), ContractError> {
        let source = self.existing(template)?;
        if self.storage.exists(project_file) {
            return Err(project_codes::ALREADY_EXISTS.with("path", project_file.display()));
        }
        let mut project = self.storage.load(&source)?;
        if let Some(stem) = project_file.file_stem() {
            project.name = stem.to_string_lossy().into_owned();
        }
        self.storage.save(project_file, &project)?;
        self.files.load(project_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_contracts::{
        AutoPoint, AutomationClip, AutomationLane, Clip, ClipContent, InstrumentSpec, MAX_CLIP_LOOP_TICKS, Marker, ParamTarget, SampleRef,
        Track,
    };

    fn sampler(sample: f32) -> InstrumentSpec {
        let mut spec = InstrumentSpec::new("sampler", [("sample".to_string(), sample), ("gain".to_string(), 0.5)].into());
        spec.frozen = Some(9);
        spec
    }

    fn song() -> Project {
        let mut project = Project::empty();
        project.tracks = vec![Track::new(1, "Kick", Some(sampler(20.0)), 36), Track::new(2, "Voice", None, 60)];
        project.samples = [20, 21]
            .map(|id| SampleRef { id, name: format!("s{id}"), path: format!("/p/samples/s{id}.wav").into(), markers: Vec::new() })
            .to_vec();
        project.clips = vec![Clip {
            id: 30,
            track: 2,
            start_tick: 0,
            length_ticks: 960,
            content: ClipContent::audio(21),
            gain: 1.0,
            fade_in_ticks: 0,
            fade_out_ticks: 0,
            fade_in_shape: Default::default(),
            fade_out_shape: Default::default(),
            offset_ticks: 0,
            offset_frames: 0,
            warp: None,
            pitch_semitones: 0.0,
        }];
        project.automation = vec![AutomationLane {
            id: 40,
            target: ParamTarget::NodeVolume { node: 0 },
            clips: vec![AutomationClip::once(1_000, 0, MAX_CLIP_LOOP_TICKS, vec![AutoPoint::new(0, 1.0)])],
        }];
        project.loop_region = Some((0, 3840));
        project.markers = vec![Marker { id: 50, tick: 0, name: "Intro".into() }];
        project
    }

    #[test]
    fn structure_keeps_tracks_and_instrument_sounds_only() {
        let params = HashMap::from([("sampler", vec!["sample"])]);
        let structure = structure_of(&song(), &params);
        assert_eq!(structure.tracks.len(), 2, "tracks stay");
        let spec = structure.tracks[0].instrument.as_ref().unwrap();
        assert_eq!((spec.params["sample"], spec.params["gain"], spec.frozen), (20.0, 0.5, None), "parameters stay, freezing does not");
        assert!(structure.clips.is_empty() && structure.automation.is_empty() && structure.markers.is_empty());
        assert_eq!(structure.loop_region, None);
        assert_eq!(structure.samples.iter().map(|s| s.id).collect::<Vec<_>>(), [20], "the sampler sound stays, the clip sound does not");
        assert_eq!(structure.graph, song().graph);
    }
}
