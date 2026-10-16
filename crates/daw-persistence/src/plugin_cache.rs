//! The remembered plugin list: `<state folder>/plugins.json`.
//! The document format is known by its writer (`daw-vst3`); this only stores the text safely.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::settings;
use daw_contracts::{AppLog, ContractError, PluginCacheStore};

use crate::atomic::write_atomic;
use crate::json_state::warn;

const FILE_NAME: &str = "plugins.json";

pub struct PluginCacheFile {
    file: PathBuf,
    log: Arc<dyn AppLog>,
}

impl PluginCacheFile {
    pub fn new(state_dir: &Path, log: Arc<dyn AppLog>) -> Self {
        Self { file: state_dir.join(FILE_NAME), log }
    }
}

impl PluginCacheStore for PluginCacheFile {
    /// No file means there was no scan yet; an unreadable one means the same to the app, but it is logged.
    fn load(&self) -> Option<String> {
        match fs::read_to_string(&self.file) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                warn(&*self.log, &self.file, &format!("the plugin list was not read: {error}"));
                None
            }
        }
    }

    fn save(&self, text: &str) -> Result<(), ContractError> {
        write_atomic(&self.file, text.as_bytes(), settings::STATE_NOT_WRITTEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};

    fn cache(dir: &Path) -> PluginCacheFile {
        PluginCacheFile::new(dir, Arc::new(MemoryLog::default()))
    }

    #[test]
    fn missing_file_is_none_and_saved_text_comes_back() {
        let dir = temp_dir("plugin-cache-roundtrip");
        let cache = cache(&dir);
        assert_eq!(cache.load(), None);
        cache.save("first").unwrap();
        cache.save("second").unwrap();
        assert_eq!(cache.load().as_deref(), Some("second"));
        assert!(!dir.join("plugins.tmp").exists(), "the temporary file is gone");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unwritable_location_is_an_error_not_a_panic() {
        let dir = temp_dir("plugin-cache-blocked");
        fs::write(dir.join("state"), "a file instead of a folder").unwrap();
        assert!(cache(&dir.join("state")).save("x").unwrap_err().is(settings::STATE_NOT_WRITTEN));
        fs::remove_dir_all(dir).ok();
    }
}
