//! Autosave into the project folder (`Autosaver`, a background thread) and recovery after a crash (`ProjectAutosave`).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use daw_contracts::codes::project as codes;
use daw_contracts::{AppLog, AutosaveControl, ContractError, LogEntry, ProjectIo, ProjectStorage, ProjectView, sync::lock};

use crate::layout::ProjectLayout;

/// Autosave of the current project: the file `autosave/autosave.dawproj` in the project folder.
pub struct ProjectAutosave {
    project: Arc<dyn ProjectIo>,
    log: Arc<dyn AppLog>,
    /// A series of failed deletions is going on: only the first is logged (the thread deletes the file every half minute).
    discard_failing: AtomicBool,
}

impl ProjectAutosave {
    pub fn new(project: Arc<dyn ProjectIo>, log: Arc<dyn AppLog>) -> Self {
        Self { project, log, discard_failing: AtomicBool::new(false) }
    }

    fn file(&self) -> Option<PathBuf> {
        self.project.path().map(|p| ProjectLayout::from_file(&p).autosave_file())
    }
}

impl AutosaveControl for ProjectAutosave {
    fn pending(&self) -> Option<PathBuf> {
        self.file().filter(|f| f.is_file())
    }

    fn restore(&self) -> Result<(), ContractError> {
        let file = self.pending().ok_or_else(|| codes::AUTOSAVE_NOT_FOUND.err())?;
        self.project.restore_from(&file)?;
        self.discard();
        Ok(())
    }

    /// No file means nothing had to be deleted; any other failure is logged (one entry per series).
    fn discard(&self) {
        let Some(file) = self.file() else { return };
        match std::fs::remove_file(&file) {
            Err(error) if error.kind() != ErrorKind::NotFound => {
                if !self.discard_failing.swap(true, Ordering::AcqRel) {
                    let entry = LogEntry::warn("autosave", format!("the old autosave was not deleted: {error}"));
                    self.log.record(entry.with_details(format!("file: {}", file.display())));
                }
            }
            _ => self.discard_failing.store(false, Ordering::Release),
        }
    }
}

/// How often to autosave now; `None` means off. Asked on every step of the thread, so a setting change takes effect
/// at once, without a restart.
pub type AutosaveInterval = Box<dyn Fn() -> Option<Duration> + Send>;

/// How often the thread wakes up to check the setting: an interval change (or turning it on) waits no longer than this.
const RECHECK: Duration = Duration::from_secs(1);

/// A background thread: once per `interval()` it saves the project if it has unsaved edits.
/// A write failure goes to the log (one entry per series of consecutive failures): otherwise edits would stop being
/// saved silently, and the user would only learn about it when data is lost.
/// The autosave file is deleted when the project is clean (saved), including on a normal shutdown (`Drop`).
/// If unsaved edits remain on exit, the file stays: they can be brought back on the next start.
/// While autosave is off, the thread does nothing.
pub struct Autosaver {
    stop: Arc<(Mutex<bool>, Condvar)>,
    handle: Option<JoinHandle<()>>,
    autosave: Arc<ProjectAutosave>,
}

impl Autosaver {
    pub fn start(
        project: Arc<dyn ProjectIo>,
        view: Arc<dyn ProjectView>,
        storage: Arc<dyn ProjectStorage>,
        log: Arc<dyn AppLog>,
        interval: AutosaveInterval,
    ) -> Self {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let autosave = Arc::new(ProjectAutosave::new(project, Arc::clone(&log)));
        let handle = {
            let (stop, autosave) = (Arc::clone(&stop), Arc::clone(&autosave));
            thread::spawn(move || {
                let mut failing = false;
                let mut last = Instant::now();
                loop {
                    let current = interval();
                    let wait = current.map_or(RECHECK, |every| every.saturating_sub(last.elapsed()).min(RECHECK));
                    if stopped_within(&stop, wait) {
                        return;
                    }
                    let Some(every) = interval() else {
                        last = Instant::now();
                        continue;
                    };
                    if last.elapsed() < every {
                        continue;
                    }
                    last = Instant::now();
                    let Some(file) = autosave.file() else { continue };
                    if autosave.project.is_dirty() {
                        match save(&*storage, &file, &*view) {
                            Ok(()) => failing = false,
                            Err(error) if !failing => {
                                failing = true;
                                log_failure(&*log, &file, &error);
                            }
                            Err(_) => {}
                        }
                    } else {
                        autosave.discard();
                    }
                }
            })
        };
        Self { stop, handle: Some(handle), autosave }
    }
}

/// Waits for `wait` or the stop signal; `true` means it is time to stop.
fn stopped_within(stop: &(Mutex<bool>, Condvar), wait: Duration) -> bool {
    let (stopped, signal) = stop;
    let guard = lock(stopped);
    let (guard, _) = signal.wait_timeout_while(guard, wait, |stopped| !*stopped).unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard
}

fn save(storage: &dyn ProjectStorage, file: &Path, view: &dyn ProjectView) -> Result<(), ContractError> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| codes::FOLDER_NOT_CREATED.with("path", dir.display()).with("details", e))?;
    }
    storage.save_quick(file, &view.snapshot())
}

fn log_failure(log: &dyn AppLog, file: &Path, error: &ContractError) {
    log.record(LogEntry::error("autosave", format!("autosave was not written: {error}")).with_details(format!("file: {}", file.display())));
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        {
            let (stopped, signal) = &*self.stop;
            *lock(stopped) = true;
            signal.notify_all();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        if !self.autosave.project.is_dirty() {
            self.autosave.discard();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonStorage;
    use crate::testing::{MemoryLog, temp_dir};
    use daw_contracts::{LogLevel, Project};
    use std::sync::atomic::AtomicUsize;

    fn storage() -> JsonStorage {
        JsonStorage::new(Arc::new(MemoryLog::default()))
    }

    fn autosave(project: &Arc<FakeProject>) -> ProjectAutosave {
        ProjectAutosave::new(project.clone(), Arc::new(MemoryLog::default()))
    }

    fn start(project: &Arc<FakeProject>, storage: Arc<dyn ProjectStorage>, log: Arc<MemoryLog>, interval: Duration) -> Autosaver {
        Autosaver::start(project.clone(), project.clone(), storage, log, Box::new(move || Some(interval)))
    }

    /// A storage that cannot write: the disk is full, no permissions.
    struct BrokenStorage;

    impl ProjectStorage for BrokenStorage {
        fn save(&self, path: &Path, _: &Project) -> Result<(), ContractError> {
            Err(codes::FILE_NOT_WRITTEN.with("path", path.display()).with("details", "disk full"))
        }
        fn save_quick(&self, path: &Path, _: &Project) -> Result<(), ContractError> {
            Err(codes::FILE_NOT_WRITTEN.with("path", path.display()).with("details", "disk full"))
        }
        fn load(&self, path: &Path) -> Result<Project, ContractError> {
            Err(codes::FILE_NOT_READ.with("path", path.display()).with("details", "no file"))
        }
        fn exists(&self, _: &Path) -> bool {
            false
        }
    }

    struct FakeProject {
        dirty: AtomicBool,
        snapshots: AtomicUsize,
        file: PathBuf,
        restored: Mutex<Option<PathBuf>>,
    }

    impl ProjectIo for FakeProject {
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
        fn restore_from(&self, file: &Path) -> Result<(), ContractError> {
            *self.restored.lock().unwrap() = Some(file.to_path_buf());
            Ok(())
        }
        fn folder(&self) -> Option<PathBuf> {
            self.file.parent().map(Path::to_path_buf)
        }
        fn project_name(&self) -> String {
            "t".into()
        }
        fn path(&self) -> Option<PathBuf> {
            Some(self.file.clone())
        }
        fn is_dirty(&self) -> bool {
            self.dirty.load(Ordering::Acquire)
        }
        fn take_warnings(&self) -> Vec<ContractError> {
            Vec::new()
        }
    }

    impl ProjectView for FakeProject {
        fn revision(&self) -> u64 {
            0
        }
        fn snapshot(&self) -> Project {
            self.snapshots.fetch_add(1, Ordering::AcqRel);
            Project::empty()
        }
    }

    fn fake(name: &str) -> Arc<FakeProject> {
        Arc::new(FakeProject {
            dirty: AtomicBool::new(false),
            snapshots: AtomicUsize::new(0),
            file: temp_dir(&format!("autosave-{name}")).join("p.dawproj"),
            restored: Mutex::new(None),
        })
    }

    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..300 {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn saves_when_dirty_drops_the_file_when_clean_and_cleans_up_on_exit() {
        let project = fake("cycle");
        let control = autosave(&project);
        let saver = start(&project, Arc::new(storage()), Arc::new(MemoryLog::default()), Duration::from_millis(20));

        thread::sleep(Duration::from_millis(100));
        assert!(control.pending().is_none(), "a clean project is not saved");

        project.dirty.store(true, Ordering::Release);
        assert!(wait_until(|| control.pending().is_some()));
        assert!(
            control.pending().unwrap().starts_with(project.folder().unwrap().join("autosave")),
            "the autosave lies in the project folder"
        );

        project.dirty.store(false, Ordering::Release);
        assert!(wait_until(|| control.pending().is_none()), "after the project is saved the file is removed");

        project.dirty.store(true, Ordering::Release);
        assert!(wait_until(|| control.pending().is_some()));
        drop(saver);
        assert!(control.pending().is_some(), "exiting with unsaved edits keeps the autosave");

        let saver = start(&project, Arc::new(storage()), Arc::new(MemoryLog::default()), Duration::from_secs(60));
        project.dirty.store(false, Ordering::Release);
        drop(saver);
        assert!(control.pending().is_none(), "exiting after a save removes the autosave");
        std::fs::remove_dir_all(project.folder().unwrap()).ok();
    }

    #[test]
    fn the_interval_is_read_live_and_zero_turns_autosave_off() {
        let project = fake("live");
        let control = autosave(&project);
        let every = Arc::new(AtomicUsize::new(0));
        let source = every.clone();
        let interval: AutosaveInterval =
            Box::new(move || Some(source.load(Ordering::Acquire)).filter(|ms| *ms > 0).map(|ms| Duration::from_millis(ms as u64)));
        let saver = Autosaver::start(project.clone(), project.clone(), Arc::new(storage()), Arc::new(MemoryLog::default()), interval);
        project.dirty.store(true, Ordering::Release);
        thread::sleep(Duration::from_millis(150));
        assert!(control.pending().is_none(), "autosave that is off does not write");

        every.store(20, Ordering::Release);
        assert!(wait_until(|| control.pending().is_some()), "turning it on works without a restart (within RECHECK)");
        every.store(0, Ordering::Release);
        thread::sleep(Duration::from_millis(50));
        let before = project.snapshots.load(Ordering::Acquire);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(project.snapshots.load(Ordering::Acquire), before, "no more snapshots after turning it off");
        drop(saver);
        std::fs::remove_dir_all(project.folder().unwrap()).ok();
    }

    #[test]
    fn a_failing_autosave_is_logged_once_per_series() {
        let project = fake("broken");
        let log = Arc::new(MemoryLog::default());
        let saver = start(&project, Arc::new(BrokenStorage), log.clone(), Duration::from_millis(10));
        project.dirty.store(true, Ordering::Release);
        assert!(wait_until(|| project.snapshots.load(Ordering::Acquire) >= 4), "several attempts in a row");
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1, "a series of failures is one entry, not one per attempt");
        assert_eq!((entries[0].source.as_str(), entries[0].level), ("autosave", LogLevel::Error));
        assert!(entries[0].message.contains("disk full"));
        drop(saver);
        std::fs::remove_dir_all(project.folder().unwrap()).ok();
    }

    #[test]
    fn leftover_autosave_can_be_restored_or_discarded() {
        let project = fake("recover");
        let control = autosave(&project);
        assert!(control.pending().is_none());
        assert!(control.restore().unwrap_err().is(codes::AUTOSAVE_NOT_FOUND), "there is nothing to restore");

        let file = ProjectLayout::from_file(&project.file).autosave_file();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        storage().save_quick(&file, &Project::empty()).unwrap();
        assert_eq!(control.pending(), Some(file.clone()));

        control.restore().unwrap();
        assert_eq!(project.restored.lock().unwrap().as_deref(), Some(file.as_path()));
        assert!(control.pending().is_none(), "after restoring the file is removed");

        storage().save_quick(&file, &Project::empty()).unwrap();
        control.discard();
        assert!(control.pending().is_none());
        std::fs::remove_dir_all(project.folder().unwrap()).ok();
    }

    #[test]
    fn a_missing_file_is_not_a_failure_but_a_stuck_one_is_logged_once() {
        let project = fake("discard");
        let log = Arc::new(MemoryLog::default());
        let control = ProjectAutosave::new(project.clone(), log.clone());
        control.discard();
        assert!(log.recent(10).is_empty(), "no file means nothing to delete");

        let file = ProjectLayout::from_file(&project.file).autosave_file();
        std::fs::create_dir_all(file.join("inside")).unwrap();
        control.discard();
        control.discard();
        let entries = log.recent(10);
        assert_eq!(entries.len(), 1, "a series of failures is one entry");
        assert_eq!((entries[0].source.as_str(), entries[0].level), ("autosave", LogLevel::Warn));
        std::fs::remove_dir_all(project.folder().unwrap()).ok();
    }
}
