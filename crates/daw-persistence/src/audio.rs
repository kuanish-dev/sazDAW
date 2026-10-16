//! `AudioSettingsFile` (`AudioSettingsStore`): "remember the audio device choice between runs" in `audio.json`
//! next to the rest of the app state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::settings as codes;
use daw_contracts::{AppLog, AudioSettings, AudioSettingsStore, ContractError};

use crate::atomic::write_json;
use crate::json_state::read_or_default;

const FILE_NAME: &str = "audio.json";

pub struct AudioSettingsFile {
    file: PathBuf,
    log: Arc<dyn AppLog>,
}

impl AudioSettingsFile {
    pub fn new(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        Self { file: state_dir.join(FILE_NAME), log }
    }
}

impl AudioSettingsStore for AudioSettingsFile {
    /// No file gives the default choice; a damaged one too, but logged: audio must start in any case.
    fn load(&self) -> AudioSettings {
        read_or_default(&self.file, &*self.log)
    }

    fn save(&self, settings: &AudioSettings) -> Result<(), ContractError> {
        write_json(&self.file, settings, codes::STATE_NOT_WRITTEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use std::fs;

    fn store(dir: &Path) -> AudioSettingsFile {
        AudioSettingsFile::new(dir, Arc::new(MemoryLog::default()))
    }

    #[test]
    fn saved_choice_is_read_back_by_a_new_store_with_camel_case_keys() {
        let dir = temp_dir("audio-roundtrip");
        assert_eq!(store(&dir).load(), AudioSettings::default(), "no file gives the defaults");
        let chosen = AudioSettings {
            output: Some("Speakers".into()),
            input: Some("Microphone".into()),
            buffer_frames: Some(256),
            record_latency_ms: Some(12.5),
        };
        store(&dir).save(&chosen).unwrap();
        assert_eq!(store(&dir).load(), chosen);
        let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("\"bufferFrames\": 256") && text.contains("\"recordLatencyMs\": 12.5"), "{text}");
        store(&dir).save(&AudioSettings::default()).unwrap();
        assert_eq!(store(&dir).load(), AudioSettings::default(), "a reset to the defaults is saved too");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn corrupt_file_gives_defaults_is_logged_and_can_be_overwritten() {
        let dir = temp_dir("audio-corrupt");
        fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        let log = Arc::new(MemoryLog::default());
        let store = AudioSettingsFile::new(&dir, log.clone());
        assert_eq!(store.load(), AudioSettings::default());
        assert_eq!(log.recent(10).len(), 1, "a damaged file is logged");
        let chosen = AudioSettings { output: Some("A".into()), ..AudioSettings::default() };
        store.save(&chosen).unwrap();
        assert_eq!(store.load(), chosen);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unknown_or_missing_fields_do_not_break_loading() {
        let dir = temp_dir("audio-partial");
        fs::write(dir.join(FILE_NAME), r#"{ "bufferFrames": 512, "extra": 1 }"#).unwrap();
        assert_eq!(store(&dir).load(), AudioSettings { buffer_frames: Some(512), ..AudioSettings::default() });
        fs::remove_dir_all(dir).ok();
    }
}
