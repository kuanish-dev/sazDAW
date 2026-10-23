//! `ProjectSampleCatalog`: knows which sounds the project has — importing, listing and removing samples.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{
    AssetStore, ClipContent, ContractError, InstrumentCatalog, ParamKind, SampleCatalog, SampleDecoder, SampleId, SampleInfo, SampleRef,
    SampleStore, tr,
};

use crate::{ProjectService, ops, take_rules};

const MAX_SAMPLES: usize = 1024;

pub struct ProjectSampleCatalog {
    project: Arc<ProjectService>,
    store: Arc<dyn SampleStore>,
    decoder: Arc<dyn SampleDecoder>,
    assets: Arc<dyn AssetStore>,
    instruments: Arc<dyn InstrumentCatalog>,
}

impl ProjectSampleCatalog {
    pub fn new(
        project: Arc<ProjectService>,
        store: Arc<dyn SampleStore>,
        decoder: Arc<dyn SampleDecoder>,
        assets: Arc<dyn AssetStore>,
        instruments: Arc<dyn InstrumentCatalog>,
    ) -> Self {
        Self { project, store, decoder, assets, instruments }
    }

    /// Sample reference params (`ParamKind::Sample`) of each instrument kind: removing a sample resets them.
    fn sample_params(&self) -> HashMap<&'static str, Vec<&'static str>> {
        self.instruments
            .kinds()
            .into_iter()
            .map(|kind| (kind.kind, kind.params.iter().filter(|p| p.kind == ParamKind::Sample).map(|p| p.name).collect()))
            .collect()
    }
}

impl SampleCatalog for ProjectSampleCatalog {
    fn import(&self, path: &Path) -> Result<SampleInfo, ContractError> {
        let adopted = match self.project.path() {
            Some(file) => self.assets.adopt(&file, path)?,
            None => path.to_path_buf(),
        };
        let path = adopted.as_path();
        let existing = self.project.read(|p| p.samples.iter().find(|s| s.path == path).map(|s| s.id));
        if let Some(id) = existing
            && let Some(info) = self.samples().into_iter().find(|s| s.id == id)
        {
            return Ok(info);
        }

        let data = Arc::new(self.decoder.decode(path)?);
        let name = path.file_stem().map_or_else(|| tr!("sample", "сэмпл"), |s| s.to_string_lossy().into_owned());
        let id = self.project.edit(&tr!("Import sample", "Импорт сэмпла"), |project| {
            if project.samples.len() >= MAX_SAMPLES {
                return Err(samples::TOO_MANY.with("max", MAX_SAMPLES));
            }
            let id = project.alloc_id();
            self.store.insert(id, Arc::clone(&data));
            project.samples.push(SampleRef { id, name: name.clone(), path: path.to_path_buf(), markers: Vec::new() });
            Ok(id)
        })?;
        Ok(SampleInfo {
            id,
            name,
            path: path.to_path_buf(),
            frame_count: data.frame_count(),
            sample_rate: data.sample_rate,
            markers: Vec::new(),
        })
    }

    fn samples(&self) -> Vec<SampleInfo> {
        self.project.read(|p| {
            p.samples
                .iter()
                .map(|s| {
                    let data = self.store.get(s.id);
                    SampleInfo {
                        id: s.id,
                        name: s.name.clone(),
                        path: s.path.clone(),
                        frame_count: data.as_ref().map_or(0, |d| d.frame_count()),
                        sample_rate: data.as_ref().map_or(0, |d| d.sample_rate),
                        markers: s.markers.clone(),
                    }
                })
                .collect()
        })
    }

    fn remove(&self, id: SampleId) -> Result<(), ContractError> {
        let sample_params = self.sample_params();
        self.project.edit(&tr!("Delete sample", "Удалить сэмпл"), |project| {
            ops::check_sample(project, id)?;
            project.samples.retain(|s| s.id != id);
            project.clips.retain(|c| !take_rules::is_only_sound(&c.content, id));
            for clip in &mut project.clips {
                if clip.content.sample() == Some(id) || matches!(&clip.content, ClipContent::Audio { takes, .. } if takes.contains(&id)) {
                    take_rules::remove(&mut clip.content, id)?;
                }
            }
            for instrument in project.tracks.iter_mut().filter_map(|t| t.instrument.as_mut()) {
                for name in sample_params.get(instrument.kind.as_str()).into_iter().flatten() {
                    if let Some(chosen) = instrument.params.get_mut(*name)
                        && *chosen == id as f32
                    {
                        *chosen = 0.0;
                    }
                }
            }
            Ok(())
        })
    }
}
