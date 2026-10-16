//! Safe file writes: into a temporary file next to the target, then a rename. A failure mid-write leaves the previous
//! file whole instead of a fragment. Shared by the project, autosave and the small app state files.

use std::fs;
use std::path::Path;

use daw_contracts::codes::general;
use daw_contracts::{ContractError, ErrorCode};

/// Writes `bytes` to `path` atomically; the folder is created. A failure is `failed` (a code with `{path}` and
/// `{details}`), so each caller reports its own kind of file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], failed: ErrorCode) -> Result<(), ContractError> {
    let error = |e: std::io::Error| failed.with("path", path.display()).with("details", e);
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(error)?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(error)?;
    fs::rename(&temporary, path).map_err(error)
}

/// Writes `value` as readable JSON atomically (see `write_atomic`).
pub(crate) fn write_json(path: &Path, value: &impl serde::Serialize, failed: ErrorCode) -> Result<(), ContractError> {
    // Our own types always serialize; a failure here is a programming error.
    let text = serde_json::to_string_pretty(value).map_err(|e| general::INTERNAL.with("details", e))?;
    write_atomic(path, text.as_bytes(), failed)
}

#[cfg(test)]
mod tests {
    use daw_contracts::codes::settings;

    use super::*;

    #[test]
    fn writes_through_a_temporary_file_and_creates_the_folder() {
        let dir = crate::testing::temp_dir("atomic");
        let file = dir.join("deep/state.json");
        write_atomic(&file, b"one", settings::STATE_NOT_WRITTEN).unwrap();
        write_atomic(&file, b"two", settings::STATE_NOT_WRITTEN).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"two");
        assert!(!file.with_extension("tmp").exists(), "the temporary file is gone");
        let blocked = write_atomic(&file.join("inside"), b"x", settings::STATE_NOT_WRITTEN).unwrap_err();
        assert!(blocked.is(settings::STATE_NOT_WRITTEN), "{blocked}");
        fs::remove_dir_all(dir).ok();
    }
}
