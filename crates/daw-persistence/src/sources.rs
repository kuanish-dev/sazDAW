//! Linked folders for the sound browser: a list in an app state file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::settings;
use daw_contracts::{AppLog, ContractError, SoundSources};
use serde::{Deserialize, Serialize};

use crate::json_state::JsonState;

const FILE_NAME: &str = "sound-sources.json";
const MAX_SOURCES: usize = 64;

#[derive(Default, Serialize, Deserialize)]
struct State {
    folders: Vec<PathBuf>,
}

/// Links to sound folders in `<state folder>/sound-sources.json`.
pub struct SourceStore {
    state: JsonState<State>,
}

impl SourceStore {
    /// Reads the list; no file or a damaged one (logged) gives an empty list.
    pub fn open(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        Self { state: JsonState::open(state_dir, FILE_NAME, log) }
    }
}

impl SoundSources for SourceStore {
    fn list(&self) -> Vec<PathBuf> {
        self.state.lock().folders.clone()
    }

    fn add(&self, folder: &Path) -> Result<(), ContractError> {
        if !folder.is_dir() {
            return Err(settings::SOURCE_NOT_A_FOLDER.with("path", folder.display()));
        }
        let mut state = self.state.lock();
        if state.folders.iter().any(|f| f == folder) {
            return Ok(());
        }
        if state.folders.len() >= MAX_SOURCES {
            return Err(settings::TOO_MANY_SOURCES.with("max", MAX_SOURCES));
        }
        state.folders.push(folder.to_path_buf());
        self.state.persist(&state);
        Ok(())
    }

    fn remove(&self, folder: &Path) {
        let mut state = self.state.lock();
        state.folders.retain(|f| f != folder);
        self.state.persist(&state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use std::fs;

    fn log() -> Arc<dyn AppLog> {
        Arc::new(MemoryLog::default())
    }

    #[test]
    fn links_are_kept_between_runs_without_duplicates_and_removal_leaves_files() {
        let root = temp_dir("sources-links");
        let packs = root.join("Packs");
        fs::create_dir_all(&packs).unwrap();
        fs::write(packs.join("kick.wav"), b"x").unwrap();

        let store = SourceStore::open(&root.join("state"), log());
        assert_eq!(store.list(), [] as [PathBuf; 0]);
        store.add(&packs).unwrap();
        store.add(&packs).unwrap();
        assert_eq!(store.list(), std::slice::from_ref(&packs));
        assert_eq!(SourceStore::open(&root.join("state"), log()).list(), std::slice::from_ref(&packs), "survives a restart");

        store.remove(&packs);
        assert_eq!(store.list(), [] as [PathBuf; 0]);
        assert!(packs.join("kick.wav").is_file(), "the files stay in place");
        assert_eq!(SourceStore::open(&root.join("state"), log()).list(), [] as [PathBuf; 0]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn only_folders_can_be_linked_and_corrupt_state_is_ignored() {
        let root = temp_dir("sources-reject");
        fs::write(root.join("a.txt"), b"x").unwrap();
        let store = SourceStore::open(&root, log());
        assert!(store.add(&root.join("a.txt")).unwrap_err().is(settings::SOURCE_NOT_A_FOLDER));
        assert!(store.add(&root.join("missing")).unwrap_err().is(settings::SOURCE_NOT_A_FOLDER));
        fs::write(root.join(FILE_NAME), "{ not json").unwrap();
        assert_eq!(SourceStore::open(&root, log()).list(), [] as [PathBuf; 0]);
        fs::remove_dir_all(root).ok();
    }
}
