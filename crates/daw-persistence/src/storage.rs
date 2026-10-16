//! `JsonStorage` (`ProjectStorage`): "the project file format": a JSON envelope with the format version, sample paths
//! relative to the project folder, atomic writes.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use daw_contracts::codes::project as codes;
use daw_contracts::{AppLog, ContractError, LogEntry, Project, ProjectStorage};
use serde::{Deserialize, Serialize};

use crate::assets;
use crate::atomic::write_json;
use crate::layout::ProjectLayout;

/// File format version; grows with incompatible changes.
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: u32,
    project: Project,
}

/// The project in a JSON `.dawproj` file; saving gathers the sounds into the project folder.
pub struct JsonStorage {
    log: Arc<dyn AppLog>,
}

impl JsonStorage {
    pub fn new(log: Arc<dyn AppLog>) -> Self {
        Self { log }
    }

    fn write(path: &Path, project: Project) -> Result<(), ContractError> {
        write_json(path, &Envelope { format: FORMAT_VERSION, project }, codes::FILE_NOT_WRITTEN)
    }
}

impl ProjectStorage for JsonStorage {
    fn save(&self, path: &Path, project: &Project) -> Result<(), ContractError> {
        let layout = ProjectLayout::from_file(path);
        // A project folder always has `samples/`: the sound browser looks into it even while it is empty.
        let samples = layout.samples_dir();
        fs::create_dir_all(&samples).map_err(|e| codes::FOLDER_NOT_CREATED.with("path", samples.display()).with("details", e))?;
        let mut stored = project.clone();
        for sample in &mut stored.samples {
            // Samples are usually in the project folder already (the import brings them); the rest are gathered here. If one
            // is not copied, the project is still saved with a link to the original file (or the work would be lost), and the failure is logged.
            let bundled = assets::bundle(&layout, &sample.path, sample.id).unwrap_or_else(|error| {
                let message = format!(
                    "sample \"{}\" was not copied into the project folder, the link to the original file stays: {error}",
                    sample.name
                );
                self.log.record(LogEntry::warn("persist", message).with_details(format!("project file: {}", path.display())));
                sample.path.clone()
            });
            sample.path = bundled.strip_prefix(layout.root()).map_or(bundled.clone(), Path::to_path_buf);
        }
        Self::write(path, stored)
    }

    fn save_quick(&self, path: &Path, project: &Project) -> Result<(), ContractError> {
        Self::write(path, project.clone())
    }

    fn exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn load(&self, path: &Path) -> Result<Project, ContractError> {
        let text = fs::read_to_string(path).map_err(|e| codes::FILE_NOT_READ.with("path", path.display()).with("details", e))?;
        let envelope: Envelope = serde_json::from_str(&text).map_err(|e| codes::FILE_DAMAGED.with("details", e))?;
        if envelope.format > FORMAT_VERSION {
            return Err(codes::NEWER_FORMAT.with("format", envelope.format));
        }

        let dir = path.parent().unwrap_or(Path::new("."));
        let mut project = envelope.project;
        for sample in &mut project.samples {
            if sample.path.is_relative() {
                sample.path = dir.join(&sample.path);
            }
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLog, temp_dir};
    use daw_contracts::{LogLevel, SampleRef};

    fn storage() -> JsonStorage {
        JsonStorage::new(Arc::new(MemoryLog::default()))
    }

    fn sample(id: u32, path: &Path) -> SampleRef {
        SampleRef { id, name: format!("s{id}"), path: path.to_path_buf(), markers: Vec::new() }
    }

    #[test]
    fn round_trips_a_project_and_creates_the_project_folder() {
        let dir = temp_dir("storage-roundtrip");
        let mut project = Project::empty();
        project.name = "My track".into();
        project.tempo_map = project
            .tempo_map
            .with_tempos(vec![daw_contracts::TempoChange { tick: 0, bpm: daw_contracts::Bpm::new(97.5).unwrap(), ramp: false }])
            .unwrap();
        let path = dir.join("Track/Track.dawproj");
        storage().save(&path, &project).unwrap();
        assert!(!path.with_extension("tmp").exists(), "the temporary file is gone");
        assert_eq!(storage().load(&path).unwrap(), project);
        // A project folder always has `samples/`, and autosave does not create project folders.
        assert!(dir.join("Track/samples").is_dir());
        storage().save_quick(&dir.join("Track/autosave/autosave.dawproj"), &project).unwrap();
        assert!(!dir.join("Track/autosave/samples").exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn samples_are_copied_next_to_the_project_with_relative_paths() {
        let dir = temp_dir("storage-bundle");
        let source_dir = temp_dir("storage-bundle-src");
        let source = source_dir.join("kick.wav");
        fs::write(&source, b"RIFF....").unwrap();

        let mut project = Project::empty();
        project.samples.push(sample(5, &source));
        let path = dir.join("song.dawproj");
        storage().save(&path, &project).unwrap();

        assert!(dir.join("samples/kick.wav").exists());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("samples/kick.wav") && !text.contains(source.to_str().unwrap()));
        let loaded = storage().load(&path).unwrap();
        assert_eq!(loaded.samples[0].path, dir.join("samples/kick.wav"));

        // Saving again is idempotent.
        storage().save(&path, &loaded).unwrap();
        assert_eq!(storage().load(&path).unwrap().samples[0].path, dir.join("samples/kick.wav"));
        fs::remove_dir_all(dir).ok();
        fs::remove_dir_all(source_dir).ok();
    }

    #[test]
    fn missing_sample_file_keeps_the_original_path_and_is_logged_not_fatal() {
        let dir = temp_dir("storage-missing");
        let mut project = Project::empty();
        project.samples.push(sample(1, Path::new("/no/such/file.wav")));
        let path = dir.join("p.dawproj");
        let log = Arc::new(MemoryLog::default());
        JsonStorage::new(log.clone()).save(&path, &project).unwrap();
        assert_eq!(storage().load(&path).unwrap().samples[0].path, Path::new("/no/such/file.wav"));
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1, "the copy failure is logged");
        assert_eq!((entries[0].source.as_str(), entries[0].level), ("persist", LogLevel::Warn));
        assert!(entries[0].message.contains("\"s1\""), "{}", entries[0].message);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn same_name_different_content_gets_the_sample_id_prefix() {
        let dir = temp_dir("storage-collide");
        let (a, b) = (temp_dir("storage-collide-a").join("snare.wav"), temp_dir("storage-collide-b").join("snare.wav"));
        fs::write(&a, b"AAAA").unwrap();
        fs::write(&b, b"BBBB").unwrap();
        let mut project = Project::empty();
        project.samples.extend([sample(1, &a), sample(2, &b)]);
        storage().save(&dir.join("p.dawproj"), &project).unwrap();
        assert_eq!(fs::read(dir.join("samples/snare.wav")).unwrap(), b"AAAA");
        assert_eq!(fs::read(dir.join("samples/2-snare.wav")).unwrap(), b"BBBB");
        for folder in [dir, a.parent().unwrap().into(), b.parent().unwrap().into()] {
            fs::remove_dir_all(folder).ok();
        }
    }

    #[test]
    fn missing_corrupt_future_and_invalid_files_are_rejected_with_clear_errors() {
        let dir = temp_dir("storage-bad");
        let path = dir.join("bad.dawproj");
        assert!(storage().load(&path).unwrap_err().is(codes::FILE_NOT_READ));
        fs::write(&path, "{ not json").unwrap();
        assert!(storage().load(&path).unwrap_err().is(codes::FILE_DAMAGED));

        storage().save_quick(&path, &Project::empty()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replacen("\"format\": 1", "\"format\": 99", 1)).unwrap();
        assert!(storage().load(&path).unwrap_err().is(codes::NEWER_FORMAT));
        fs::write(&path, text.replacen("\"bpm\": 130.0", "\"bpm\": 5.0", 1)).unwrap();
        assert!(storage().load(&path).unwrap_err().is(codes::FILE_DAMAGED), "tempo out of range");
        fs::remove_dir_all(dir).ok();
    }
}
