//! Arrangement and presets: rendering a track to audio (`ProjectBounce` over the offline renderer), project templates
//! and instrument and effect presets (both in the app state folder).

use std::path::Path;
use std::sync::Arc;

use daw_contracts::{AssetStore, InstrumentCatalog, Presets, ProjectStorage, ProjectTemplates, TrackBounce};
use daw_persistence::{PresetFolder, PresetTargets, TemplateFolder};
use daw_project::{BounceParts, ProjectBounce};
use daw_render::OfflineExporter;
use daw_samples::WavWriter;

use super::project::ProjectParts;

pub(super) struct Arrangement {
    pub bounce: Arc<dyn TrackBounce>,
    pub templates: Arc<dyn ProjectTemplates>,
    pub presets: Arc<dyn Presets>,
}

pub(super) fn build(
    state_dir: &Path,
    project: &ProjectParts,
    renderer: Arc<OfflineExporter>,
    storage: Arc<dyn ProjectStorage>,
    assets: Arc<dyn AssetStore>,
    instruments: Arc<dyn InstrumentCatalog>,
) -> Arrangement {
    let parts = BounceParts {
        tracks: project.tracks.clone(),
        clips: project.clips.clone(),
        graph: project.graph.clone(),
        samples: project.samples.clone(),
    };
    let targets = PresetTargets {
        view: project.service.clone(),
        tracks: project.tracks.clone(),
        graph: project.graph.clone(),
        states: project.plugin_states.clone(),
        history: project.service.clone(),
    };
    Arrangement {
        bounce: Arc::new(ProjectBounce::new(project.service.clone(), renderer, Arc::new(WavWriter), assets, parts)),
        templates: Arc::new(TemplateFolder::new(state_dir, project.service.clone(), storage, project.files.clone(), instruments)),
        presets: Arc::new(PresetFolder::new(state_dir, targets)),
    }
}
