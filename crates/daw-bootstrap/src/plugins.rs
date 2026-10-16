//! Session plugins: where VST3 plugins are looked for, which `PluginScanner` goes into `Controls`, where the scan helper
//! (a separate process) is, and the plugin crash marker (`plugin-guard.bin`).

use std::path::PathBuf;
use std::sync::Arc;

use daw_catalog::{EffectRegistry, InstrumentRegistry};
use daw_contracts::{AppLog, ContractError, PluginCacheStore, PluginScanner, PreferencesStore};
use daw_persistence::PluginCacheFile;
use daw_vst3::{HELPER_NAME, PluginDirectory, ProcessMarker, Vst3Plugins};

use crate::SessionConfig;
use crate::wiring::Devices;

/// Where system VST3 plugins are looked for on macOS (`~` expands to the home folder). The only place of this list.
const VST3_DIRS: [&str; 2] = ["/Library/Audio/Plug-Ins/VST3", "~/Library/Audio/Plug-Ins/VST3"];

/// The plugin crash marker: a file in the state folder that the next start reads. The only place of the file name.
pub const GUARD_FILE: &str = "plugin-guard.bin";

/// The plugin scan helper (`daw-plugin-scan`) lies next to the app executable: in `SazDAW.app/Contents/MacOS` (put there
/// by `scripts/make-dmg.sh`) or in `target/debug|release` during development (cargo tests run from `target/debug/deps`,
/// so one folder up). Without it the scan answers with a clear error (`Vst3Plugins::scan`) instead of loading foreign
/// code into the app process.
fn scan_helper() -> PathBuf {
    let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) else {
        return PathBuf::from(HELPER_NAME);
    };
    let beside = dir.join(HELPER_NAME);
    match dir.parent() {
        Some(target) if !beside.exists() && dir.ends_with("deps") => target.join(HELPER_NAME),
        _ => beside,
    }
}

/// What session plugins need besides the catalogs: preferences (the disabled list), the crash marker, the log.
pub struct PluginDeps<'a> {
    pub preferences: &'a Arc<dyn PreferencesStore>,
    pub marker: Arc<ProcessMarker>,
    pub log: &'a Arc<dyn AppLog>,
}

fn vst3_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    VST3_DIRS
        .iter()
        .filter_map(|dir| match dir.strip_prefix("~/") {
            Some(rest) => home.as_ref().map(|home| home.join(rest)),
            None => Some(PathBuf::from(dir)),
        })
        .collect()
}

/// The session plugin scanner and the directory of loaded plugins. Plugins are NOT scanned or loaded at startup: a
/// third-party plugin may crash, and startup must be fast and safe. In a real session the catalog gets only what the
/// previous scan remembered (`plugins.json`); the scan button loads bundles in a separate process (the
/// `daw-plugin-scan` helper). The test (`Headless`) session has no plugins: the scan finds nothing and reads nothing
/// from disk, there is no cache, unless `SessionConfig::plugin_dirs` points it at folders (the dev-only fixture plugins
/// of the VST3 host tests; loaded in this process); plugins can still be disabled there (the list is in the test session
/// preferences).
pub(crate) fn scanner(
    config: &SessionConfig,
    devices: &Devices,
    effects: &Arc<EffectRegistry>,
    instruments: &Arc<InstrumentRegistry>,
    deps: PluginDeps,
) -> (Arc<dyn PluginScanner>, Arc<PluginDirectory>) {
    let log = deps.log.clone();
    let (dirs, cache): (_, Arc<dyn PluginCacheStore>) = match devices {
        Devices::System => (vst3_dirs(), Arc::new(PluginCacheFile::new(&config.state_dir, log.clone()))),
        Devices::Headless => (Vec::new(), Arc::new(NoCache)),
    };
    let dirs = config.plugin_dirs.clone().unwrap_or(dirs);
    let mut plugins = Vst3Plugins::new(dirs, effects.clone(), instruments.clone(), cache, log)
        .with_marker(deps.marker)
        .with_preferences(deps.preferences.clone());
    if matches!(devices, Devices::System) {
        plugins = plugins.with_scan_helper(scan_helper());
    }
    plugins.load_cached();
    let directory = plugins.directory();
    (Arc::new(plugins), directory)
}

/// The test session remembers nothing on disk.
struct NoCache;

impl PluginCacheStore for NoCache {
    fn load(&self) -> Option<String> {
        None
    }
    fn save(&self, _text: &str) -> Result<(), ContractError> {
        Ok(())
    }
}
