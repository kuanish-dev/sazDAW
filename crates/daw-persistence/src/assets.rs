//! `FsAssets` (`AssetStore`): "bring a sound file into the project folder", a copy in `samples/` that never overwrites other files.

use std::fs;
use std::path::{Path, PathBuf};

use daw_contracts::codes::project;
use daw_contracts::{AssetStore, ContractError};

use crate::layout::{ProjectLayout, sanitize_name};

/// Whether two files have the same content; the size is compared first so large files are not read in vain.
fn same_content(a: &Path, b: &Path) -> bool {
    let size = |path: &Path| fs::metadata(path).map(|m| m.len()).ok();
    size(a) == size(b) && fs::read(a).ok() == fs::read(b).ok()
}

/// A short number stable for a path: a duplicate name gets the same number on every save.
fn path_tag(path: &Path) -> u32 {
    path.to_string_lossy().bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))) % 10_000
}

/// Copies a file into the project's `samples/`. If the file is already there, returns it;
/// if another file with the same name is there, adds the number `tag`.
pub fn bundle(layout: &ProjectLayout, source: &Path, tag: u32) -> Result<PathBuf, ContractError> {
    let samples_dir = layout.samples_dir();
    if source.starts_with(&samples_dir) {
        return Ok(source.to_path_buf());
    }
    let file_name = source.file_name().ok_or_else(|| project::SAMPLE_NOT_A_FILE.with("path", source.display()))?;
    let mut target = samples_dir.join(file_name);
    if target.exists() && !same_content(&target, source) {
        target = samples_dir.join(format!("{tag}-{}", file_name.to_string_lossy()));
    }
    fs::create_dir_all(&samples_dir).map_err(|e| project::FOLDER_NOT_CREATED.with("path", samples_dir.display()).with("details", e))?;
    if !target.exists() {
        fs::copy(source, &target).map_err(|e| project::SAMPLE_NOT_COPIED.with("path", source.display()).with("details", e))?;
    }
    Ok(target)
}

pub struct FsAssets;

impl AssetStore for FsAssets {
    fn adopt(&self, project_file: &Path, source: &Path) -> Result<PathBuf, ContractError> {
        bundle(&ProjectLayout::from_file(project_file), source, path_tag(source))
    }

    fn new_asset_path(&self, project_file: &Path, name: &str) -> PathBuf {
        let dir = ProjectLayout::from_file(project_file).samples_dir();
        let base = sanitize_name(name);
        let mut candidate = dir.join(format!("{base}.wav"));
        let mut counter = 2;
        while candidate.exists() {
            candidate = dir.join(format!("{base} {counter}.wav"));
            counter += 1;
        }
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    #[test]
    fn adoption_copies_once_and_is_idempotent() {
        let dir = temp_dir("assets-adopt");
        let source = dir.join("kick.wav");
        fs::write(&source, b"AAAA").unwrap();
        let project = dir.join("Трек").join("Трек.dawproj");

        let first = FsAssets.adopt(&project, &source).unwrap();
        assert_eq!(first, dir.join("Трек/samples/kick.wav"));
        assert_eq!(fs::read(&first).unwrap(), b"AAAA");
        assert_eq!(FsAssets.adopt(&project, &source).unwrap(), first, "a repeat makes no extra copies");
        assert_eq!(FsAssets.adopt(&project, &first).unwrap(), first, "the file is already inside the project");
        assert_eq!(fs::read_dir(dir.join("Трек/samples")).unwrap().count(), 1);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn different_files_with_the_same_name_do_not_overwrite_each_other() {
        let dir = temp_dir("assets-collide");
        let (a, b) = (dir.join("a").join("snare.wav"), dir.join("b").join("snare.wav"));
        fs::create_dir_all(a.parent().unwrap()).unwrap();
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        fs::write(&a, b"AAAA").unwrap();
        fs::write(&b, b"BBBB").unwrap();
        let project = dir.join("P").join("P.dawproj");
        let first = FsAssets.adopt(&project, &a).unwrap();
        let second = FsAssets.adopt(&project, &b).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"AAAA");
        assert_eq!(fs::read(second).unwrap(), b"BBBB");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn new_asset_paths_are_free_and_safe() {
        let dir = temp_dir("assets-newpath");
        let project = dir.join("P/P.dawproj");
        let first = FsAssets.new_asset_path(&project, "Kick/1");
        assert_eq!(first, dir.join("P/samples/Kick_1.wav"));
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::write(&first, b"x").unwrap();
        assert_eq!(FsAssets.new_asset_path(&project, "Kick/1"), dir.join("P/samples/Kick_1 2.wav"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_sources_are_reported() {
        let dir = temp_dir("assets-missing");
        assert!(FsAssets.adopt(&dir.join("P/P.dawproj"), &dir.join("nope.wav")).unwrap_err().is(project::SAMPLE_NOT_COPIED));
        fs::remove_dir_all(dir).ok();
    }
}
