//! `ProjectKey` (`KeyControl`): the song's key — the tonic and the scale, kept in the project (`Project::key`), so it
//! is saved, undone and sent to the window like any other project data.

use std::sync::Arc;

use daw_contracts::codes::notes;
use daw_contracts::{ContractError, KeyControl, ScaleKey, tr};

use crate::ProjectService;

pub struct ProjectKey {
    project: Arc<ProjectService>,
}

impl ProjectKey {
    pub fn new(project: Arc<ProjectService>) -> Self {
        Self { project }
    }
}

impl KeyControl for ProjectKey {
    fn key(&self) -> Option<ScaleKey> {
        self.project.read(|project| project.key)
    }

    fn set_key(&self, key: Option<ScaleKey>) -> Result<(), ContractError> {
        if let Some(ScaleKey { root, .. }) = key
            && root > 11
        {
            return Err(notes::SCALE_ROOT_OUT_OF_RANGE.with("root", root));
        }
        // The same key leaves the project equal, and the core records no undo step for that.
        self.project.edit(&tr!("Song key", "Тональность"), |project| {
            project.key = key;
            Ok(())
        })
    }
}
