//! `ProjectCleaner` (`ProjectClear`): removes all project content but keeps the project itself — in one core edit, so
//! it is one undo step, and the core removes the DSP objects of the removed tracks and effects from the engine itself.

use std::sync::Arc;

use daw_contracts::{ClearOptions, ContractError, MASTER, Project, ProjectClear, TempoMap, tr};

use crate::ProjectService;

pub struct ProjectCleaner {
    project: Arc<ProjectService>,
}

impl ProjectCleaner {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

impl ProjectClear for ProjectCleaner {
    fn clear(&self, options: ClearOptions) -> Result<(), ContractError> {
        self.project.edit(&tr!("Clear project", "Очистить проект"), |project| {
            clear_content(project, options);
            Ok(())
        })
    }
}

/// Empty content: only the master (with its volume). The project name and the id counter stay, so new entities never
/// get the ids of removed ones and undo cannot mix them up.
fn clear_content(project: &mut Project, options: ClearOptions) {
    project.tracks.clear();
    project.clips.clear();
    project.loop_region = None;
    project.graph.nodes.retain(|node| node.id == MASTER);
    project.graph.edges.clear();
    project.automation.clear();
    project.markers.clear();
    project.folders.clear();
    project.midi_mappings.clear();
    if !options.keep_tempo {
        project.tempo_map = TempoMap::default();
    }
    if options.remove_samples {
        // The sounds stay in memory (the sample store is left alone): undo brings them back without reading files.
        project.samples.clear();
    }
}
