//! `Vst3CrashGuard`: "know whether the previous session broke off inside a plugin" (`PluginCrashGuard`).
//!
//! Structure: `map` is the memory-mapped marker file (`plugin-guard.bin` in the state folder); `marker` holds the
//! plugin instance slots and the "who is processing now" mark. On start the previous session's marker is read and turned into
//! a report (`PluginCrash`): which plugin, the last project from the recent list and where the plugin sits in that project.

mod map;
mod marker;

use std::path::Path;
use std::sync::{Arc, Mutex};

use daw_contracts::sync::lock;
use daw_contracts::{AppLog, NodeKind, PluginCrash, PluginCrashGuard, Project, ProjectStorage, RecentProjects, tr};

use crate::journal;
use map::{GuardMap, Mark};
pub use marker::{GuardSlot, ProcessMarker};

pub struct Vst3CrashGuard {
    marker: Arc<ProcessMarker>,
    report: Mutex<Option<PluginCrash>>,
}

impl Vst3CrashGuard {
    /// Reads the previous session's marker from `file` and starts a new one. Where to look for the project for the report: `recent` and `storage`.
    /// If the file does not open, the protection does not work (the reason is logged) and the app keeps working.
    pub fn open(file: &Path, recent: &dyn RecentProjects, storage: &dyn ProjectStorage, log: Arc<dyn AppLog>) -> Self {
        let (map, previous) = match GuardMap::open(file) {
            Ok((map, previous)) => (Some(map), previous),
            Err(error) => {
                journal::error(
                    &*log,
                    format!("the plugin crash marker ({}) did not open, the protection does not work: {error}", file.display()),
                );
                (None, None)
            }
        };
        let report = previous.map(|mark| crash_report(mark, recent, storage, &*log));
        if let Some(crash) = &report {
            journal::error(&*log, format!("the previous session broke off inside plugin \"{}\" ({})", crash.name, crash.kind));
        }
        Self { marker: Arc::new(ProcessMarker::new(map, log)), report: Mutex::new(report) }
    }

    /// Instance marks: the plugin factories get them (`Vst3Plugins::with_marker`).
    pub fn marker(&self) -> Arc<ProcessMarker> {
        self.marker.clone()
    }
}

impl PluginCrashGuard for Vst3CrashGuard {
    fn last_crash(&self) -> Option<PluginCrash> {
        lock(&self.report).clone()
    }

    fn dismiss(&self) {
        *lock(&self.report) = None;
    }

    fn end_session(&self) {
        self.marker.set_running(false);
    }
}

fn crash_report(mark: Mark, recent: &dyn RecentProjects, storage: &dyn ProjectStorage, log: &dyn AppLog) -> PluginCrash {
    let project = recent.last();
    let places = project.as_deref().map_or_else(Vec::new, |path| match storage.load(path) {
        Ok(project) => places_of(&project, &mark.kind),
        Err(error) => {
            journal::warn(log, format!("plugin crash report: project {} was not read: {error}", path.display()));
            Vec::new()
        }
    });
    PluginCrash { kind: mark.kind, name: mark.name, project, places }
}

/// Where in the project the plugin of kind `kind` sits: tracks using it as the instrument, and effect nodes.
fn places_of(project: &Project, kind: &str) -> Vec<String> {
    let tracks = project
        .tracks
        .iter()
        .filter(|track| track.instrument.as_ref().is_some_and(|spec| spec.kind == kind))
        .map(|track| tr!("track \"{}\"", "дорожка «{}»", track.name));
    let effects = project
        .graph
        .nodes
        .iter()
        .filter(|node| matches!(&node.kind, NodeKind::Effect { effect, .. } if effect == kind))
        .map(|node| tr!("effect \"{}\"", "эффект «{}»", node.name));
    tracks.chain(effects).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fakes::Log;
    use daw_contracts::ContractError;
    use std::path::PathBuf;

    struct Recent(Option<PathBuf>);

    impl RecentProjects for Recent {
        fn last(&self) -> Option<PathBuf> {
            self.0.clone()
        }
        fn list(&self) -> Vec<PathBuf> {
            self.0.iter().cloned().collect()
        }
        fn remember(&self, _: &Path) {}
        fn forget(&self, _: &Path) {}
    }

    /// A store that gives a project with one track "Bass" on `vst3:surge_xt`.
    struct Storage;

    impl ProjectStorage for Storage {
        fn save(&self, _: &Path, _: &Project) -> Result<(), ContractError> {
            Ok(())
        }
        fn save_quick(&self, _: &Path, _: &Project) -> Result<(), ContractError> {
            Ok(())
        }
        fn load(&self, path: &Path) -> Result<Project, ContractError> {
            if path.ends_with("broken.dawproj") {
                return Err(daw_contracts::codes::general::INTERNAL.with("details", "broken"));
            }
            let mut project = Project::default();
            let instrument = Some(daw_contracts::InstrumentSpec::new("vst3:surge_xt", daw_contracts::Params::new()));
            project.tracks.push(daw_contracts::Track::new(1, "Bass", instrument, 60));
            Ok(project)
        }
        fn exists(&self, _: &Path) -> bool {
            true
        }
    }

    fn crashed_file(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("daw-crashguard-{name}-{}.bin", std::process::id()));
        std::fs::remove_file(&path).ok();
        let (map, _) = GuardMap::open(&path).unwrap();
        map.write_slot(5, "vst3:surge_xt", "Surge XT");
        map.enter(5);
        path
    }

    #[test]
    fn a_crash_inside_a_plugin_is_reported_with_its_project_and_places_until_dismissed() {
        let path = crashed_file("report");
        let log = Arc::new(Log::default());
        let project = PathBuf::from("/projects/Song.dawproj");
        let guard = Vst3CrashGuard::open(&path, &Recent(Some(project.clone())), &Storage, log.clone());
        let crash = guard.last_crash().expect("the crash is found");
        assert_eq!((crash.kind.as_str(), crash.name.as_str()), ("vst3:surge_xt", "Surge XT"));
        assert_eq!((crash.project, crash.places), (Some(project), vec!["track \"Bass\"".to_string()]));
        assert!(log.recent(10).iter().any(|e| e.message.contains("Surge XT")), "the crash is logged too");
        assert!(guard.last_crash().is_some(), "the report stays the same until dismiss");
        guard.dismiss();
        assert!(guard.last_crash().is_none());

        guard.end_session();
        drop(guard);
        let guard = Vst3CrashGuard::open(&path, &Recent(None), &Storage, log);
        assert!(guard.last_crash().is_none(), "this session closed normally");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn an_unreadable_project_still_gives_a_report_without_places() {
        let path = crashed_file("broken");
        let log = Arc::new(Log::default());
        let guard = Vst3CrashGuard::open(&path, &Recent(Some("/x/broken.dawproj".into())), &Storage, log.clone());
        let crash = guard.last_crash().unwrap();
        assert!(crash.places.is_empty());
        assert_eq!(log.recent(10).len(), 2, "both the crash and the unread project are logged");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_guard_that_cannot_open_its_file_works_without_protection() {
        let log = Arc::new(Log::default());
        let guard = Vst3CrashGuard::open(Path::new("/no/such/folder/plugin-guard.bin"), &Recent(None), &Storage, log.clone());
        assert!(guard.last_crash().is_none());
        assert!(guard.marker().claim("vst3:a", "A").is_none(), "without a file there are no slots, plugins work without a marker");
        assert_eq!(log.recent(10).len(), 1);
    }
}
