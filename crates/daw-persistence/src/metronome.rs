//! `MetronomeSettingsFile` (`MetronomeSettingsStore`): "remember the metronome settings between launches" in
//! `metronome.json` next to the rest of the app state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::settings as codes;
use daw_contracts::{AppLog, ContractError, MetronomeSettings, MetronomeSettingsStore};

use crate::atomic::write_json;
use crate::json_state::read_or_default;

const FILE_NAME: &str = "metronome.json";

pub struct MetronomeSettingsFile {
    file: PathBuf,
    log: Arc<dyn AppLog>,
}

impl MetronomeSettingsFile {
    pub fn new(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        Self { file: state_dir.join(FILE_NAME), log }
    }
}

impl MetronomeSettingsStore for MetronomeSettingsFile {
    fn load(&self) -> MetronomeSettings {
        read_or_default(&self.file, &*self.log)
    }

    fn save(&self, settings: &MetronomeSettings) -> Result<(), ContractError> {
        write_json(&self.file, settings, codes::STATE_NOT_WRITTEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use std::fs;

    fn store(dir: &Path) -> MetronomeSettingsFile {
        MetronomeSettingsFile::new(dir, Arc::new(MemoryLog::default()))
    }

    #[test]
    fn the_metronome_is_off_until_turned_on_and_a_new_store_reads_the_choice_back() {
        let dir = temp_dir("metronome-roundtrip");
        assert_eq!(store(&dir).load(), MetronomeSettings { enabled: false }, "off on the first launch");
        store(&dir).save(&MetronomeSettings { enabled: true }).unwrap();
        assert_eq!(store(&dir).load(), MetronomeSettings { enabled: true });
        let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("\"enabled\": true"), "{text}");
        store(&dir).save(&MetronomeSettings { enabled: false }).unwrap();
        assert_eq!(store(&dir).load(), MetronomeSettings { enabled: false }, "turning it off is remembered too");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_damaged_file_gives_the_defaults_and_is_logged() {
        let dir = temp_dir("metronome-corrupt");
        fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        let log = Arc::new(MemoryLog::default());
        let store = MetronomeSettingsFile::new(&dir, log.clone());
        assert_eq!(store.load(), MetronomeSettings::default());
        assert_eq!(log.recent(10).len(), 1, "a damaged file is logged");
        store.save(&MetronomeSettings { enabled: true }).unwrap();
        assert_eq!(store.load(), MetronomeSettings { enabled: true }, "and can be overwritten");
        fs::remove_dir_all(dir).ok();
    }
}
