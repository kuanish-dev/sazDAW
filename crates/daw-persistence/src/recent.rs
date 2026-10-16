//! Recent projects: a small app state file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::{AppLog, RecentProjects};
use serde::{Deserialize, Serialize};

use crate::json_state::JsonState;

const MAX_RECENT: usize = 12;
const FILE_NAME: &str = "recent.json";

#[derive(Default, Serialize, Deserialize)]
struct State {
    recent: Vec<PathBuf>,
}

/// The list of recent projects in `<state folder>/recent.json`.
pub struct RecentStore {
    state: JsonState<State>,
}

impl RecentStore {
    /// Reads the list; no file or a damaged one (logged) gives an empty list.
    pub fn open(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        Self { state: JsonState::open(state_dir, FILE_NAME, log) }
    }
}

impl RecentProjects for RecentStore {
    fn last(&self) -> Option<PathBuf> {
        self.list().into_iter().next()
    }

    fn list(&self) -> Vec<PathBuf> {
        self.state.lock().recent.iter().filter(|p| p.is_file()).cloned().collect()
    }

    fn remember(&self, project_file: &Path) {
        let mut state = self.state.lock();
        state.recent.retain(|p| p != project_file);
        state.recent.insert(0, project_file.to_path_buf());
        state.recent.truncate(MAX_RECENT);
        self.state.persist(&state);
    }

    fn forget(&self, project_file: &Path) {
        let mut state = self.state.lock();
        state.recent.retain(|p| p != project_file);
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

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "x").unwrap();
        path
    }

    #[test]
    fn remembers_most_recent_first_without_duplicates_and_survives_restart() {
        let dir = temp_dir("recent-order");
        let (a, b) = (touch(&dir, "a.dawproj"), touch(&dir, "b.dawproj"));
        let store = RecentStore::open(&dir.join("state"), log());
        assert_eq!(store.last(), None);
        store.remember(&a);
        store.remember(&b);
        store.remember(&a);
        assert_eq!(store.list(), [a.clone(), b.clone()]);
        assert_eq!(store.last(), Some(a.clone()));

        let reopened = RecentStore::open(&dir.join("state"), log());
        assert_eq!(reopened.list(), [a, b]);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_files_are_hidden_and_forget_removes_entries() {
        let dir = temp_dir("recent-missing");
        let (a, b) = (touch(&dir, "a.dawproj"), touch(&dir, "b.dawproj"));
        let store = RecentStore::open(&dir, log());
        store.remember(&a);
        store.remember(&b);
        fs::remove_file(&b).unwrap();
        assert_eq!(store.list(), std::slice::from_ref(&a), "a deleted file is not shown");
        assert_eq!(store.last(), Some(a.clone()));
        store.forget(&a);
        assert_eq!(store.last(), None);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn corrupt_state_is_ignored_and_the_list_is_capped() {
        let dir = temp_dir("recent-corrupt");
        fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        let store = RecentStore::open(&dir, log());
        assert_eq!(store.list(), [] as [PathBuf; 0]);
        for i in 0..20 {
            store.remember(&touch(&dir, &format!("p{i}.dawproj")));
        }
        assert_eq!(store.list().len(), MAX_RECENT);
        fs::remove_dir_all(dir).ok();
    }
}
