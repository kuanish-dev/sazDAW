//! Scanning through the real `daw-plugin-scan` helper (a separate process) on the real plugin
//! fixture `test-plugins/distortion` (dev-only), and the crash marker around its processing. No plugin — the test reports
//! the reason and finishes without an error.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "integration test: a panic with a clear message is the check"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::support::{Cache, DISTORTION, Effects, Instruments, MemoryLog};
use daw_contracts::{
    AppLog, ContractError, EffectFactory, Params, PluginCrashGuard, PluginScanner, Project, ProjectStorage, RecentProjects, StreamConfig,
};
use daw_vst3::{IsolatedScan, SCAN_TIMEOUT, Vst3CrashGuard, Vst3Plugins};

const HELPER: &str = env!("CARGO_BIN_EXE_daw-plugin-scan");

fn distortion() -> Option<&'static Path> {
    let path = Path::new(DISTORTION);
    if path.exists() {
        Some(path)
    } else {
        eprintln!("SKIPPED: no {DISTORTION} (build it: scripts/build-test-plugins.sh)");
        None
    }
}

#[test]
fn the_helper_scans_a_real_bundle_in_its_own_process() {
    let Some(bundle) = distortion() else { return };
    let log = Arc::new(MemoryLog::default());
    let outcome = IsolatedScan::new(PathBuf::from(HELPER), SCAN_TIMEOUT).scan(bundle, &*log);
    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    let plugin = outcome.plugins.iter().find(|p| p.name == "Distortion").expect("Distortion is found");
    assert!(!plugin.params.is_empty() && plugin.params_total >= plugin.params.len());
    assert_eq!(plugin.path, bundle);
}

struct NoRecent;

impl RecentProjects for NoRecent {
    fn last(&self) -> Option<PathBuf> {
        None
    }
    fn list(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn remember(&self, _: &Path) {}
    fn forget(&self, _: &Path) {}
}

struct NoStorage;

impl ProjectStorage for NoStorage {
    fn save(&self, _: &Path, _: &Project) -> Result<(), ContractError> {
        Ok(())
    }
    fn save_quick(&self, _: &Path, _: &Project) -> Result<(), ContractError> {
        Ok(())
    }
    fn load(&self, _: &Path) -> Result<Project, ContractError> {
        Err(daw_contracts::codes::general::INTERNAL.with("details", "no storage"))
    }
    fn exists(&self, _: &Path) -> bool {
        false
    }
}

/// The app's full path: a scan by the helper, an effect from the catalog with a marker slot, processing, and a "crash" after it —
/// the marker must not blame a plugin that has already left processing.
#[test]
fn a_guarded_effect_leaves_no_mark_once_it_is_out_of_processing() {
    let Some(bundle) = distortion() else { return };
    let dir = std::env::temp_dir().join(format!("daw-vst3-isolated-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let link = dir.join("Distortion.vst3");
    std::fs::remove_file(&link).ok();
    std::os::unix::fs::symlink(bundle, &link).unwrap();
    let guard_file = dir.join("plugin-guard.bin");
    let log: Arc<dyn AppLog> = Arc::new(MemoryLog::default());

    let guard = Vst3CrashGuard::open(&guard_file, &NoRecent, &NoStorage, log.clone());
    let effects = Arc::new(Effects::default());
    let plugins =
        Vst3Plugins::new(vec![dir.clone()], effects.clone(), Arc::new(Instruments::default()), Arc::new(Cache::default()), log.clone())
            .with_scan_helper(PathBuf::from(HELPER))
            .with_marker(guard.marker());
    let report = plugins.scan().unwrap();
    assert_eq!(report.effects, 1, "{report:?}");
    let factory: Box<dyn EffectFactory> = effects.0.lock().unwrap().pop().unwrap();
    let mut effect = factory.create(&Params::new(), None).unwrap();
    effect.prepare(StreamConfig { sample_rate: 48_000, channels: 2 });
    let mut block = vec![0.25_f32; 2 * 256];
    effect.process(&mut block, None);
    drop(guard);

    let next = Vst3CrashGuard::open(&guard_file, &NoRecent, &NoStorage, log);
    assert_eq!(next.last_crash(), None, "the plugin already left processing: nothing to blame it for");
    drop(effect);
    std::fs::remove_dir_all(dir).ok();
}
