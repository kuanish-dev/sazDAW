//! The whole project: file lifecycle (`ProjectIo`), reading the whole project (`ProjectView`), the file format
//! (`ProjectStorage`), the starter beat (`DemoProject`) and clearing the contents (`ProjectClear`).

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{ContractError, Project};

/// File lifecycle of the project. Job: which file is open; create, open, save, restore.
pub trait ProjectIo: Send + Sync {
    /// An empty untitled project without a file.
    fn new_project(&self);
    /// Saves the project to `path` and binds it to that file ("Save as" also changes the project name).
    fn save(&self, path: &Path) -> Result<(), ContractError>;
    /// Opens a project; the undo history is cleared. Missing sample files do not block it, see `take_warnings`.
    fn load(&self, path: &Path) -> Result<(), ContractError>;
    /// Creates a new empty project and saves it to `project_file` (the folder is created). An existing file is
    /// `ErrorKind::Invalid`.
    fn create(&self, project_file: &Path) -> Result<(), ContractError>;
    /// Loads the contents from `file`, but the project stays bound to its previous file and is marked modified
    /// (restoring from an autosave).
    fn restore_from(&self, file: &Path) -> Result<(), ContractError>;
    /// Project folder (next to the `.dawproj` file).
    fn folder(&self) -> Option<PathBuf>;
    fn project_name(&self) -> String;
    /// The file the project was last loaded from or saved to.
    fn path(&self) -> Option<PathBuf>;
    /// Whether there are unsaved edits.
    fn is_dirty(&self) -> bool;
    /// Warnings of the last load (for example, missing sample files) as coded errors; the list is cleared.
    fn take_warnings(&self) -> Vec<ContractError>;
}

/// Reading the whole project. Job: hand out a consistent copy of the whole project and tell whether it has changed.
/// For those who look at the project as a whole (export, autosave, the window's feed) rather than edit a part of it.
pub trait ProjectView: Send + Sync {
    /// A copy of the whole project, tempo and time signature included (`Project::tempo_map`; the transport holds no
    /// tempo).
    fn snapshot(&self) -> Project;
    /// Project version counter: grows with every published change.
    fn revision(&self) -> u64;
}

/// Project storage on disk (the file format).
pub trait ProjectStorage: Send + Sync {
    fn save(&self, path: &Path, project: &Project) -> Result<(), ContractError>;
    /// A quick write without copying files (autosave): sample paths stay as they are.
    fn save_quick(&self, path: &Path, project: &Project) -> Result<(), ContractError>;
    fn load(&self, path: &Path) -> Result<Project, ContractError>;
    fn exists(&self, path: &Path) -> bool;
}

/// A starter beat for a new project: tracks, clips and graph built with the same operations a user would use.
pub trait DemoProject: Send + Sync {
    /// Adds the beat to the current project. Afterwards the undo history is cleared and the project counts as unsaved
    /// but unmodified. A failure midway is an error, and the project returns to its previous state.
    fn fill_demo(&self) -> Result<(), ContractError>;
}

/// What to keep when clearing. JSON (RPC `project.clear`): `{ keepTempo?, removeSamples? }`, both `false` by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ClearOptions {
    /// Keep the tempo map (tempo and time signature with their changes); `false` means the new project's tempo and
    /// time signature.
    pub keep_tempo: bool,
    /// Also remove the project's samples (files on disk stay); `false` keeps the samples in the project.
    pub remove_samples: bool,
}

/// Clearing the project. Job: remove all project contents while keeping the project itself (file, name, master).
///
/// Contract: removes tracks (with instruments and plugin states), clips, buses, effects (on the master too),
/// connections, automation, markers, folders and the loop; the master and its volume stay; tempo depends on
/// `keep_tempo`, samples on `remove_samples`. The project file and its binding do not change (the project becomes
/// modified). Everything is **one undo step**: `undo` brings the whole project back. An empty project creates no undo
/// step. There are no rule errors.
pub trait ProjectClear: Send + Sync {
    fn clear(&self, options: ClearOptions) -> Result<(), ContractError>;
}
