//! The project workspace: project folder, recent projects, autosave.
//!
//! A project is a folder: it holds the `<name>.dawproj` file and everything that belongs to it (`samples/`,
//! `autosave/`, `exports/`). On launch the app opens the last project.

use std::path::{Path, PathBuf};

use crate::{ContractError, LibraryEntry};

/// Brings an external file into the project folder so the project does not depend on the original location.
pub trait AssetStore: Send + Sync {
    /// Copies `source` into the folder of project `project_file` (repeated calls are safe) and returns the copy's path.
    fn adopt(&self, project_file: &Path, source: &Path) -> Result<PathBuf, ContractError>;
    /// A free path `<name>.wav` in the project's sounds folder for a new file (the name is sanitized; if taken, a
    /// number is added).
    fn new_asset_path(&self, project_file: &Path, name: &str) -> PathBuf;
}

/// The list of recent projects the app remembers between launches.
pub trait RecentProjects: Send + Sync {
    /// The last opened project, if its file still exists.
    fn last(&self) -> Option<PathBuf>;
    /// Recent projects, newest first; missing files are not shown.
    fn list(&self) -> Vec<PathBuf>;
    fn remember(&self, project_file: &Path);
    fn forget(&self, project_file: &Path);
}

/// Autosave of the current project (kept in its folder) and recovery after a crash.
pub trait AutosaveControl: Send + Sync {
    /// An autosave file left over from the previous run.
    fn pending(&self) -> Option<PathBuf>;
    /// Loads the autosave into the current project: the path stays the same and the project is marked modified.
    fn restore(&self) -> Result<(), ContractError>;
    fn discard(&self);
}

/// Extensions of audio files the app can read.
pub const AUDIO_EXTENSIONS: [&str; 7] = ["wav", "mp3", "flac", "ogg", "aif", "aiff", "m4a"];

/// Linked folders to take sounds from conveniently (e.g. a folder of packs). Files are neither copied nor deleted:
/// a link only opens the folder in the sound browser. The list is shared by all projects.
pub trait SoundSources: Send + Sync {
    fn list(&self) -> Vec<PathBuf>;
    /// Adds a folder to the list; repeats do not create duplicates. An error if the path is not a folder.
    fn add(&self, folder: &Path) -> Result<(), ContractError>;
    /// Removes the link; the folder itself and its files stay as they were.
    fn remove(&self, folder: &Path);
}

/// Browsing sounds: the project folder and linked folders; everything else on disk is closed to the browser.
pub trait ProjectSounds: Send + Sync {
    /// Project folder.
    fn root(&self) -> Option<PathBuf>;
    /// Directories and audio files in folder `dir`; a folder outside the project and the links is an error.
    fn list(&self, dir: &Path) -> Result<Vec<LibraryEntry>, ContractError>;
}
