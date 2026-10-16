//! `FolderSounds`: the "sound browser" shows folders and audio files inside the project folder and linked folders
//! (`ProjectSounds`); the rest of the disk is closed to it: it never goes above these folders, and symlinks do not lead
//! outside.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::codes::samples;
use daw_contracts::{AUDIO_EXTENSIONS, ContractError, LibraryEntry, ProjectIo, ProjectSounds, SoundSources};

pub struct FolderSounds {
    project: Arc<dyn ProjectIo>,
    sources: Arc<dyn SoundSources>,
}

impl FolderSounds {
    pub fn new(project: Arc<dyn ProjectIo>, sources: Arc<dyn SoundSources>) -> Self {
        Self { project, sources }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

impl ProjectSounds for FolderSounds {
    fn root(&self) -> Option<PathBuf> {
        self.project.folder()
    }

    fn list(&self, dir: &Path) -> Result<Vec<LibraryEntry>, ContractError> {
        let target = fs::canonicalize(dir).map_err(|e| samples::FOLDER_OPEN_FAILED.with("path", dir.display()).with("details", e))?;
        let allowed = self.root().into_iter().chain(self.sources.list()).any(|base| {
            // A link to a vanished folder simply grants no access.
            fs::canonicalize(base).is_ok_and(|base| target.starts_with(base))
        });
        if !allowed {
            return Err(samples::FOLDER_NOT_ALLOWED.with("path", dir.display()));
        }
        let mut entries = Vec::new();
        let read = fs::read_dir(dir).map_err(|e| samples::FOLDER_READ_FAILED.with("path", dir.display()).with("details", e))?;
        for entry in read.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = dir.join(&name);
            let is_dir = path.is_dir();
            if is_dir || is_audio(&path) {
                entries.push(LibraryEntry { name, path, is_dir });
            }
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A project opened from file `self.0` (the browser needs nothing else).
    struct At(PathBuf);

    /// Linked folders in memory.
    #[derive(Default)]
    struct Links(Mutex<Vec<PathBuf>>);

    impl SoundSources for Links {
        fn list(&self) -> Vec<PathBuf> {
            self.0.lock().unwrap().clone()
        }
        fn add(&self, folder: &Path) -> Result<(), ContractError> {
            self.0.lock().unwrap().push(folder.to_path_buf());
            Ok(())
        }
        fn remove(&self, folder: &Path) {
            self.0.lock().unwrap().retain(|f| f != folder);
        }
    }

    impl ProjectIo for At {
        fn new_project(&self) {}
        fn save(&self, _: &Path) -> Result<(), ContractError> {
            Ok(())
        }
        fn load(&self, _: &Path) -> Result<(), ContractError> {
            Ok(())
        }
        fn create(&self, _: &Path) -> Result<(), ContractError> {
            Ok(())
        }
        fn restore_from(&self, _: &Path) -> Result<(), ContractError> {
            Ok(())
        }
        fn folder(&self) -> Option<PathBuf> {
            self.0.parent().map(Path::to_path_buf)
        }
        fn project_name(&self) -> String {
            String::new()
        }
        fn path(&self) -> Option<PathBuf> {
            Some(self.0.clone())
        }
        fn is_dirty(&self) -> bool {
            false
        }
        fn take_warnings(&self) -> Vec<ContractError> {
            Vec::new()
        }
    }

    fn setup(name: &str) -> (PathBuf, FolderSounds, Arc<Links>) {
        let root = std::env::temp_dir().join(format!("daw-sounds-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("P/samples")).unwrap();
        let project = Arc::new(At(root.join("P/P.dawproj")));
        let sources = Arc::new(Links::default());
        (root.clone(), FolderSounds::new(project, sources.clone()), sources)
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"RIFF").unwrap();
    }

    #[test]
    fn listing_stays_inside_the_project_folder_and_links() {
        let (root, sounds, _) = setup("list");
        touch(&root.join("P/samples/kick.wav"));
        touch(&root.join("P/samples/notes.txt"));
        touch(&root.join("P/samples/.hidden.wav"));
        fs::create_dir_all(root.join("P/samples/Drums")).unwrap();

        let names: Vec<_> = sounds.list(&root.join("P/samples")).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Drums", "kick.wav"]);
        assert!(sounds.list(&root.join("P")).is_ok(), "the project root is open");
        assert!(matches!(sounds.list(&root), Err(e) if e.is(samples::FOLDER_NOT_ALLOWED)), "a folder above the project is closed");
        assert!(sounds.list(&root.join("P/samples/../..")).is_err(), "escaping through .. is closed");
        assert!(sounds.list(Path::new("/")).is_err());
        assert!(matches!(sounds.list(&root.join("P/nope")), Err(e) if e.is(samples::FOLDER_OPEN_FAILED)));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_linked_folder_opens_with_its_subfolders_until_the_link_is_removed() {
        let (root, sounds, sources) = setup("link");
        let packs = root.join("Downloads/808 Pack");
        touch(&packs.join("Kicks/kick1.wav"));
        assert!(sounds.list(&packs).is_err(), "closed without a link");

        sources.add(&packs).unwrap();
        assert_eq!(sounds.list(&packs).unwrap()[0].name, "Kicks");
        assert_eq!(sounds.list(&packs.join("Kicks")).unwrap()[0].name, "kick1.wav", "subfolders of a link are open");
        assert!(sounds.list(&root.join("Downloads")).is_err(), "the link's parent is closed");

        sources.remove(&packs);
        assert!(sounds.list(&packs).is_err());
        assert!(packs.join("Kicks/kick1.wav").is_file(), "files are untouched");
        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_allowed_folders_is_not_followed() {
        let (root, sounds, sources) = setup("symlink");
        touch(&root.join("outside/secret.wav"));
        std::os::unix::fs::symlink(root.join("outside"), root.join("P/samples/link")).unwrap();
        assert!(sounds.list(&root.join("P/samples/link")).is_err());

        let packs = root.join("packs");
        fs::create_dir_all(&packs).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), packs.join("escape")).unwrap();
        sources.add(&packs).unwrap();
        assert!(sounds.list(&packs.join("escape")).is_err());
        fs::remove_dir_all(root).ok();
    }
}
