//! `ProjectFiles`: the project's file life cycle — create, open, save, restore from autosave.
//! Implements `ProjectIo` on top of `ProjectService` (the model owner), the storage and the recent list.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::project;
use daw_contracts::{ContractError, Project, ProjectIo, ProjectStorage, RecentProjects};

use crate::ProjectService;

pub struct ProjectFiles {
    service: Arc<ProjectService>,
    storage: Arc<dyn ProjectStorage>,
    recent: Arc<dyn RecentProjects>,
}

impl ProjectFiles {
    pub fn new(service: Arc<ProjectService>, storage: Arc<dyn ProjectStorage>, recent: Arc<dyn RecentProjects>) -> Self {
        Self { service, storage, recent }
    }
}

impl ProjectIo for ProjectFiles {
    fn new_project(&self) {
        self.service.reset();
    }

    fn save(&self, path: &Path) -> Result<(), ContractError> {
        self.service.save_with(path, |project| self.storage.save(path, project))?;
        self.recent.remember(path);
        Ok(())
    }

    fn load(&self, path: &Path) -> Result<(), ContractError> {
        let project = self.storage.load(path)?;
        self.service.install(project, Some(path.to_path_buf()), false);
        self.recent.remember(path);
        Ok(())
    }

    fn create(&self, project_file: &Path) -> Result<(), ContractError> {
        if self.storage.exists(project_file) {
            return Err(project::ALREADY_EXISTS.with("path", project_file.display()));
        }
        let mut project = Project::empty();
        if let Some(stem) = project_file.file_stem() {
            project.name = stem.to_string_lossy().into_owned();
        }
        self.storage.save(project_file, &project)?;
        self.service.install(project, Some(project_file.to_path_buf()), false);
        self.recent.remember(project_file);
        Ok(())
    }

    fn restore_from(&self, file: &Path) -> Result<(), ContractError> {
        let project = self.storage.load(file)?;
        let path = self.service.path();
        self.service.install(project, path, true);
        Ok(())
    }

    fn folder(&self) -> Option<PathBuf> {
        self.service.path().as_deref().and_then(Path::parent).map(Path::to_path_buf)
    }

    fn project_name(&self) -> String {
        self.service.name()
    }

    fn path(&self) -> Option<PathBuf> {
        self.service.path()
    }

    fn is_dirty(&self) -> bool {
        self.service.is_dirty()
    }

    fn take_warnings(&self) -> Vec<ContractError> {
        self.service.take_warnings()
    }
}
