//! `PreferencesFile` (`PreferencesStore`): "remember the user's preferences" in `preferences.json` next to the rest of
//! the app state.

use std::path::Path;
use std::sync::Arc;

use daw_contracts::{AppLog, ContractError, Preferences, PreferencesStore, i18n};

use crate::json_state::{JsonState, warn};

const FILE_NAME: &str = "preferences.json";

pub struct PreferencesFile {
    state: JsonState<Preferences>,
}

impl PreferencesFile {
    /// Reads the file: none gives the defaults; damaged or out of range (edited by hand) gives the defaults too,
    /// with a log entry: preferences are a convenience, the app must start in any case.
    pub fn open(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        let state = JsonState::<Preferences>::open(state_dir, FILE_NAME, log.clone());
        {
            let mut stored = state.lock();
            match stored.clone().checked() {
                Ok(valid) => *stored = valid,
                Err(error) => {
                    warn(&*log, state.file(), &format!("preferences are out of range, using the defaults: {error}"));
                    *stored = Preferences::default();
                }
            }
            i18n::set_current(stored.language);
        }
        Self { state }
    }
}

impl PreferencesStore for PreferencesFile {
    fn get(&self) -> Preferences {
        self.state.lock().clone()
    }

    /// Disk first, then memory: if the write fails, `get` returns what is really saved.
    fn set(&self, preferences: Preferences) -> Result<Preferences, ContractError> {
        let checked = preferences.checked()?;
        let mut stored = self.state.lock();
        self.state.try_persist(&checked)?;
        *stored = checked.clone();
        i18n::set_current(checked.language);
        Ok(checked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use daw_contracts::codes::settings;
    use daw_contracts::{Theme, TrackHeight};
    use std::fs;

    fn open(dir: &Path) -> (PreferencesFile, Arc<MemoryLog>) {
        let log = Arc::new(MemoryLog::default());
        (PreferencesFile::open(dir, log.clone()), log)
    }

    #[test]
    fn set_normalizes_saves_and_survives_restart() {
        let dir = temp_dir("preferences-roundtrip");
        let (store, log) = open(&dir);
        assert_eq!(store.get(), Preferences::default());
        assert!(log.recent(10).is_empty(), "no file gives the defaults quietly");
        let mut prefs = Preferences::default();
        prefs.profile.author = "  Ann ".into();
        prefs.autosave_seconds = 0;
        prefs.appearance.theme = Theme::Dark;
        prefs.appearance.track_height = TrackHeight::Tall;
        prefs.appearance.ui_scale = 1.25;
        let saved = store.set(prefs).unwrap();
        assert_eq!(saved.profile.author, "Ann", "returns the saved form");
        assert_eq!(store.get(), saved);
        assert_eq!(open(&dir).0.get(), saved, "a new start reads the same");
        let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("\"autosaveSeconds\": 0") && text.contains("\"trackHeight\": \"tall\""), "{text}");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn invalid_values_are_rejected_and_nothing_changes() {
        let dir = temp_dir("preferences-invalid");
        let (store, _) = open(&dir);
        let prefs = Preferences { autosave_seconds: 5, ..Preferences::default() };
        assert!(store.set(prefs).unwrap_err().is(settings::AUTOSAVE_OUT_OF_RANGE));
        assert_eq!(store.get(), Preferences::default());
        assert!(!dir.join(FILE_NAME).exists(), "invalid values are not written");
    }

    #[test]
    fn corrupt_or_out_of_range_file_gives_defaults_and_is_logged() {
        let dir = temp_dir("preferences-corrupt");
        fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        let (store, log) = open(&dir);
        assert_eq!(store.get(), Preferences::default());
        assert_eq!(log.recent(10).len(), 1, "a damaged file is logged");

        fs::write(dir.join(FILE_NAME), r#"{ "autosaveSeconds": 3, "appearance": { "theme": "light" } }"#).unwrap();
        let (store, log) = open(&dir);
        assert_eq!(store.get(), Preferences::default(), "out of range gives all defaults");
        assert_eq!(log.recent(10).len(), 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_failed_write_is_an_error_and_keeps_the_previous_value() {
        let dir = temp_dir("preferences-blocked");
        // A folder in place of the file: writing is impossible.
        fs::create_dir_all(dir.join(FILE_NAME)).unwrap();
        let (store, _) = open(&dir);
        let mut prefs = Preferences::default();
        prefs.profile.author = "Ann".into();
        assert!(store.set(prefs).unwrap_err().is(settings::STATE_NOT_WRITTEN));
        assert_eq!(store.get(), Preferences::default());
        fs::remove_dir_all(dir).ok();
    }
}
