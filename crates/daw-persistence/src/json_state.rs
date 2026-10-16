//! Small app state in a JSON file (recent projects, sound folders): reading, locking, writing.
//! It is a convenience, not project data: a failure does not stop the work but goes to the log so the cause can be found.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use daw_contracts::codes::settings;
use daw_contracts::{AppLog, ContractError, LogEntry, sync::lock};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::atomic::write_json;

/// Reads a JSON state file. No file gives the default state quietly; a damaged or unreadable one gives
/// the default state and a log entry: it is a convenience, not data, so the work goes on.
pub(crate) fn read_or_default<T: Default + DeserializeOwned>(file: &Path, log: &dyn AppLog) -> T {
    match fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
            warn(log, file, &format!("the state file is damaged, starting empty: {error}"));
            T::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(error) => {
            warn(log, file, &format!("the state file was not read, starting empty: {error}"));
            T::default()
        }
    }
}

pub(crate) struct JsonState<T> {
    file: PathBuf,
    state: Mutex<T>,
    log: Arc<dyn AppLog>,
}

impl<T: Default + Serialize + DeserializeOwned> JsonState<T> {
    /// Reads the file (see `read_or_default`).
    pub(crate) fn open(state_dir: &Path, file_name: &str, log: Arc<dyn AppLog>) -> Self {
        let file = state_dir.join(file_name);
        let state = read_or_default(&file, &*log);
        Self { file, state: Mutex::new(state), log }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        lock(&self.state)
    }

    /// Writes the state atomically; a failure is logged and does not stop the work.
    pub(crate) fn persist(&self, state: &T) {
        if let Err(error) = self.try_persist(state) {
            warn(&*self.log, &self.file, &error.to_string());
        }
    }

    /// Writes the state atomically and returns the failure to the caller: for state the user changes explicitly
    /// and must learn that the change was not saved.
    pub(crate) fn try_persist(&self, state: &T) -> Result<(), ContractError> {
        write_json(&self.file, state, settings::STATE_NOT_WRITTEN)
    }

    pub(crate) fn file(&self) -> &Path {
        &self.file
    }
}

/// A state file failure (source `persist`).
pub(crate) fn warn(log: &dyn AppLog, file: &Path, message: &str) {
    log.record(LogEntry::warn("persist", message).with_details(format!("file: {}", file.display())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use daw_contracts::LogLevel;
    use serde::Deserialize;

    #[derive(Default, Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        values: Vec<u32>,
    }

    #[test]
    fn a_missing_file_is_silent_and_a_corrupt_one_is_logged() {
        let dir = temp_dir("json-state-open");
        let log = Arc::new(MemoryLog::default());
        let fresh = JsonState::<Sample>::open(&dir, "s.json", log.clone());
        assert_eq!(*fresh.lock(), Sample::default());
        assert!(log.recent(10).is_empty(), "no file is not an event");

        fs::write(dir.join("s.json"), "{ not json").unwrap();
        let corrupt = JsonState::<Sample>::open(&dir, "s.json", log.clone());
        assert_eq!(*corrupt.lock(), Sample::default());
        let entries = log.recent(10);
        assert_eq!((entries.len(), entries[0].source.as_str(), entries[0].level), (1, "persist", LogLevel::Warn));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn persist_round_trips_and_a_write_failure_is_logged_not_fatal() {
        let dir = temp_dir("json-state-persist");
        let log = Arc::new(MemoryLog::default());
        let state = JsonState::<Sample>::open(&dir, "s.json", log.clone());
        state.persist(&Sample { values: vec![1, 2] });
        assert_eq!(*JsonState::<Sample>::open(&dir, "s.json", log.clone()).lock(), Sample { values: vec![1, 2] });
        assert!(log.recent(10).is_empty());

        // A folder in place of the file: writing is impossible.
        let blocked = JsonState::<Sample>::open(&dir, "blocked", log.clone());
        fs::create_dir_all(dir.join("blocked")).unwrap();
        blocked.persist(&Sample::default());
        assert_eq!(log.recent(10).len(), 1, "a write failure is logged");
        fs::remove_dir_all(dir).ok();
    }
}
