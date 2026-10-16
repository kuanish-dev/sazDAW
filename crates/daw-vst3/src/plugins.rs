//! `Vst3Plugins`: "find VST3 effects and instruments when the user asks and make them available in the catalog".
//!
//! Implements `PluginScanner`. The scan result is remembered in a cache (`PluginCacheStore`, file `plugins.json`),
//! and on app start the factories are raised from the cache (`load_cached`) without loading the plugins themselves:
//! a third-party plugin may crash while loading, and startup must be fast and safe.
//!
//! During a scan third-party bundles are loaded by a helper in a separate process (`with_scan_helper`, `isolated.rs`): a plugin
//! that crashes or hangs while loading becomes a line in `failed`, not an app crash. Without the helper (tests
//! only) bundles load in this process. Disabled plugins (`set_disabled`, the list lives in user preferences)
//! stay in the catalog hidden: they are not offered, and a project using them opens with a clear warning.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use daw_contracts::codes::{general, plugins};
use daw_contracts::sync::lock;
use daw_contracts::{
    AppLog, ContractError, EffectGroups, InstalledPlugin, InstrumentGroups, PluginCacheStore, PluginScanner, PluginType, PreferencesStore,
    ScanReport,
};
use serde::{Deserialize, Serialize};

use crate::catalog::factories_of;
use crate::directory::PluginDirectory;
use crate::guard::ProcessMarker;
use crate::isolated::{IsolatedScan, SCAN_TIMEOUT};
use crate::journal;
use crate::scanner::{PluginInfo, PluginKind, ScanOutcome, Vst3Scanner};

/// The effect and instrument catalog group this crate owns.
const GROUP: &str = "vst3";

#[derive(Serialize, Deserialize)]
struct CacheDocument {
    plugins: Vec<PluginInfo>,
}

pub struct Vst3Plugins {
    dirs: Vec<PathBuf>,
    /// What is in the catalog now: the plugins the user found (from the cache or the last scan).
    listing: Mutex<Vec<PluginInfo>>,
    effects: Arc<dyn EffectGroups>,
    instruments: Arc<dyn InstrumentGroups>,
    cache: Arc<dyn PluginCacheStore>,
    log: Arc<dyn AppLog>,
    /// Catalog kind -> plugin: plugin windows find what to open by it.
    directory: Arc<PluginDirectory>,
    /// Two scans at once make no sense: the second waits for the first.
    scanning: Mutex<()>,
    /// The scan helper in a separate process; `None` means bundles load in this process (tests only).
    isolated: Option<IsolatedScan>,
    /// "Plugin is processing" marks for the crash report; every catalog instance gets a slot.
    marker: Arc<ProcessMarker>,
    /// Where the list of disabled plugins is kept; `None` means plugins cannot be disabled (tests).
    preferences: Option<Arc<dyn PreferencesStore>>,
}

impl Vst3Plugins {
    pub fn new(
        dirs: Vec<PathBuf>,
        effects: Arc<dyn EffectGroups>,
        instruments: Arc<dyn InstrumentGroups>,
        cache: Arc<dyn PluginCacheStore>,
        log: Arc<dyn AppLog>,
    ) -> Self {
        Self {
            dirs,
            listing: Mutex::new(Vec::new()),
            effects,
            instruments,
            cache,
            directory: Arc::new(PluginDirectory::default()),
            scanning: Mutex::new(()),
            isolated: None,
            marker: Arc::new(ProcessMarker::in_memory(log.clone())),
            preferences: None,
            log,
        }
    }

    /// During a scan third-party bundles are loaded by the `helper` (`daw-plugin-scan`) in a separate process.
    #[must_use]
    pub fn with_scan_helper(mut self, helper: PathBuf) -> Self {
        self.isolated = Some(IsolatedScan::new(helper, SCAN_TIMEOUT));
        self
    }

    /// The crash marker where plugin instances are marked (`Vst3CrashGuard::marker`).
    #[must_use]
    pub fn with_marker(mut self, marker: Arc<ProcessMarker>) -> Self {
        self.marker = marker;
        self
    }

    /// The preferences store with the list of disabled plugins (`Preferences::disabled_plugins`).
    #[must_use]
    pub fn with_preferences(mut self, preferences: Arc<dyn PreferencesStore>) -> Self {
        self.preferences = Some(preferences);
        self
    }

    /// The "kind -> plugin" directory for plugin windows; updated together with the catalog.
    pub fn directory(&self) -> Arc<PluginDirectory> {
        self.directory.clone()
    }

    fn listing(&self) -> MutexGuard<'_, Vec<PluginInfo>> {
        lock(&self.listing)
    }

    /// Both groups are replaced whole. Returns how many effects and instruments got into the catalog.
    fn publish(&self, listing: &[PluginInfo]) -> (usize, usize) {
        let counts = (
            listing.iter().filter(|info| info.kind == PluginKind::Effect).count(),
            listing.iter().filter(|info| info.kind == PluginKind::Instrument).count(),
        );
        let previous: Vec<_> = self.directory.entries().into_iter().map(|(_, source)| source).collect();
        let (effects, instruments, sources) = factories_of(listing.to_vec(), &previous, &self.disabled(), &self.marker, &self.log);
        self.directory.replace(sources);
        self.effects.replace_group(GROUP, effects);
        self.instruments.replace_group(GROUP, instruments);
        counts
    }

    /// App start: puts the cached plugins in the catalog. Plugin files are not opened.
    /// No cache means no plugins; a broken cache is logged and also gives "no plugins".
    pub fn load_cached(&self) {
        let Some(text) = self.cache.load() else { return };
        match serde_json::from_str::<CacheDocument>(&text) {
            Ok(document) => {
                let mut listing = self.listing();
                *listing = document.plugins;
                self.publish(&listing);
            }
            Err(error) => journal::warn(&*self.log, format!("the plugin list (plugins.json) is damaged, a new scan is needed: {error}")),
        }
    }

    fn disabled(&self) -> HashSet<String> {
        self.preferences.as_ref().map(|store| store.get().disabled_plugins.into_iter().collect()).unwrap_or_default()
    }

    /// All bundles of the scan folders: each in the helper (if set), one failing does not stop the others.
    fn scan_folders(&self) -> Result<ScanOutcome, ContractError> {
        let scanner = Vst3Scanner::new(self.dirs.clone(), self.log.clone());
        let Some(isolated) = &self.isolated else { return Ok(scanner.scan()) };
        if !isolated.helper().is_file() {
            return Err(plugins::SCAN_HELPER_MISSING.with("path", isolated.helper().display()));
        }
        let mut outcome = ScanOutcome::default();
        for bundle in scanner.bundles() {
            outcome.merge(isolated.scan(&bundle, &*self.log));
        }
        Ok(outcome)
    }

    fn remember(&self, plugins: &[PluginInfo]) {
        let result = serde_json::to_string_pretty(&CacheDocument { plugins: plugins.to_vec() })
            .map_err(|error| general::INTERNAL.with("details", format!("could not encode the plugin list: {error}")))
            .and_then(|text| self.cache.save(&text));
        // Without the cache plugins are available until the app closes, so the scan is not failed, but the failure is not hidden.
        if let Err(error) = result {
            journal::error(&*self.log, format!("could not remember the plugin list: {error}"));
        }
    }
}

impl PluginScanner for Vst3Plugins {
    fn scan(&self) -> Result<ScanReport, ContractError> {
        let _one_at_a_time = lock(&self.scanning);
        let outcome = self.scan_folders()?;
        self.remember(&outcome.plugins);
        let mut listing = self.listing();
        *listing = outcome.plugins;
        let (effects, instruments) = self.publish(&listing);
        Ok(ScanReport { found: effects + instruments, effects, instruments, failed: outcome.failed })
    }

    /// From the "kind -> plugin" directory: it holds exactly what is in the catalogs now, with the same kinds.
    fn installed(&self) -> Vec<InstalledPlugin> {
        let disabled = self.disabled();
        let mut plugins: Vec<InstalledPlugin> = self
            .directory
            .entries()
            .into_iter()
            .map(|(kind, source)| {
                let off = disabled.contains(&kind);
                installed_plugin(kind, source.info(), off)
            })
            .collect();
        plugins.sort_by_cached_key(|p| (p.plugin_type != PluginType::Instrument, p.name.to_lowercase(), p.kind.clone()));
        plugins
    }

    fn folders(&self) -> Vec<PathBuf> {
        self.dirs.clone()
    }

    fn set_disabled(&self, kind: &str, disabled: bool) -> Result<(), ContractError> {
        let Some(store) = &self.preferences else {
            return Err(plugins::DISABLED_LIST_UNAVAILABLE.err());
        };
        if disabled && self.directory.find(kind).is_none() {
            return Err(plugins::NOT_FOUND.with("kind", kind));
        }
        let mut preferences = store.get();
        let list = &mut preferences.disabled_plugins;
        if list.iter().any(|k| k == kind) == disabled {
            return Ok(());
        }
        if disabled {
            list.push(kind.to_string());
        } else {
            list.retain(|k| k != kind);
        }
        store.set(preferences)?;
        // The catalog is rebuilt: a disabled plugin hides, an enabled one is offered again (bundles are not reloaded).
        self.publish(&self.listing());
        Ok(())
    }
}

fn installed_plugin(kind: String, info: &PluginInfo, disabled: bool) -> InstalledPlugin {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    InstalledPlugin {
        kind,
        name: info.name.clone(),
        vendor: info.vendor.clone(),
        path: info.path.clone(),
        plugin_type: match info.kind {
            PluginKind::Effect => PluginType::Effect,
            PluginKind::Instrument => PluginType::Instrument,
        },
        params: count(info.params.len()),
        params_total: count(info.params_total),
        disabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fakes::{Log, fake_helper};
    use daw_contracts::{EffectFactory, InstrumentFactory, L10n, ParamKind, Params, Preferences};

    #[derive(Default)]
    struct Groups(Mutex<Vec<Box<dyn EffectFactory>>>);

    impl EffectGroups for Groups {
        fn replace_group(&self, group: &str, factories: Vec<Box<dyn EffectFactory>>) {
            assert_eq!(group, GROUP);
            *self.0.lock().unwrap() = factories;
        }
    }

    #[derive(Default)]
    struct InstrumentSet(Mutex<Vec<Box<dyn InstrumentFactory>>>);

    impl InstrumentGroups for InstrumentSet {
        fn replace_group(&self, group: &str, factories: Vec<Box<dyn InstrumentFactory>>) {
            assert_eq!(group, GROUP);
            *self.0.lock().unwrap() = factories;
        }
    }

    impl InstrumentSet {
        fn kinds(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().iter().map(|f| f.kind()).collect()
        }
    }

    impl Groups {
        fn kinds(&self) -> Vec<&'static str> {
            self.0.lock().unwrap().iter().map(|f| f.kind()).collect()
        }
    }

    #[derive(Default)]
    struct Cache(Mutex<Option<String>>);

    impl PluginCacheStore for Cache {
        fn load(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn save(&self, text: &str) -> Result<(), ContractError> {
            *self.0.lock().unwrap() = Some(text.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct Prefs(Mutex<Preferences>);

    impl PreferencesStore for Prefs {
        fn get(&self) -> Preferences {
            self.0.lock().unwrap().clone()
        }
        fn set(&self, preferences: Preferences) -> Result<Preferences, ContractError> {
            *self.0.lock().unwrap() = preferences.clone();
            Ok(preferences)
        }
    }

    struct Rig {
        plugins: Vst3Plugins,
        effects: Arc<Groups>,
        instruments: Arc<InstrumentSet>,
        cache: Arc<Cache>,
        prefs: Arc<Prefs>,
        log: Arc<Log>,
    }

    fn rig(dirs: Vec<PathBuf>) -> Rig {
        let (effects, instruments) = (Arc::new(Groups::default()), Arc::new(InstrumentSet::default()));
        let (cache, log, prefs) = (Arc::new(Cache::default()), Arc::new(Log::default()), Arc::new(Prefs::default()));
        let plugins =
            Vst3Plugins::new(dirs, effects.clone(), instruments.clone(), cache.clone(), log.clone()).with_preferences(prefs.clone());
        Rig { plugins, effects, instruments, cache, prefs, log }
    }

    const CACHED: &str = r#"{"plugins":[
        {"path":"/no/such/Fuzz.vst3","class_id":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16],"name":"Fuzz Box","vendor":"Someone",
         "kind":"Effect","params":[{"id":7,"label":"Drive, dB","shape":"Number","choices":[],"min":0.0,"max":40.0,"default":10.0,"default_normalized":0.25}],"params_total":1},
        {"path":"/no/such/Synth.vst3","class_id":[16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1],"name":"Big Synth","vendor":"Someone",
         "kind":"Instrument","params":[{"id":3,"label":"Mode","shape":"Choice","choices":["Low","High"],"min":0.0,"max":1.0,"default":1.0,"default_normalized":1.0}],"params_total":900}]}"#;

    #[test]
    fn cached_plugins_appear_in_their_own_catalogs_without_loading_any_bundle() {
        let rig = rig(Vec::new());
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        assert_eq!(rig.effects.kinds(), ["vst3:fuzz_box"]);
        assert_eq!(rig.instruments.kinds(), ["vst3:big_synth"]);
        let factory = &rig.effects.0.lock().unwrap()[0];
        assert_eq!(factory.label().get(), "Fuzz Box (VST3)");
        let drive = &factory.describe()[0];
        assert_eq!((drive.label.get(), drive.min, drive.max, drive.default), ("Drive, dB", 0.0, 40.0, 10.0), "real units from the cache");
        let synth = &rig.instruments.0.lock().unwrap()[0];
        assert_eq!(synth.label().get(), "Big Synth (VST3)");
        let mode = &synth.describe()[0];
        assert_eq!(
            (mode.name, mode.default, mode.kind, mode.choices),
            ("p3", 1.0, ParamKind::Choice, &[L10n::same("Low"), L10n::same("High")][..])
        );
        assert!(synth.rebuild_on().is_empty());
        assert!(rig.log.recent(10).is_empty(), "the bundle was not touched: the plugin file is missing anyway, but there is no failure");
    }

    #[test]
    fn republishing_an_unchanged_plugin_keeps_its_source() {
        let rig = rig(Vec::new());
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        let before = rig.plugins.directory.find("vst3:fuzz_box").unwrap();
        rig.plugins.load_cached();
        let after = rig.plugins.directory.find("vst3:fuzz_box").unwrap();
        assert!(Arc::ptr_eq(&before, &after), "the same source: the bundle is not loaded again, strings do not leak again");
        assert_eq!(rig.effects.0.lock().unwrap()[0].label(), before.label());
    }

    #[test]
    fn plugin_gone_from_disk_gives_a_clear_error_and_a_log_entry() {
        let rig = rig(Vec::new());
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        let error = rig.effects.0.lock().unwrap()[0].create(&Params::new(), None).err().expect("no plugin, an error");
        assert!(error.is(plugins::LOAD_FAILED) && error.to_string().contains("gone from disk"), "{error}");
        let error = rig.instruments.0.lock().unwrap()[0].create(&Params::new(), None).err().expect("no plugin, an error");
        assert!(error.is(plugins::LOAD_FAILED) && error.to_string().contains("gone from disk"), "{error}");
        let entries = rig.log.recent(10);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.source == "vst3"));
    }

    #[test]
    fn installed_lists_the_catalog_instruments_first_then_effects() {
        let rig = rig(vec![PathBuf::from("/plugins")]);
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();

        let installed = rig.plugins.installed();
        let order: Vec<(&str, PluginType)> = installed.iter().map(|p| (p.kind.as_str(), p.plugin_type)).collect();
        assert_eq!(order, [("vst3:big_synth", PluginType::Instrument), ("vst3:fuzz_box", PluginType::Effect)]);
        let synth = &installed[0];
        assert_eq!((synth.name.as_str(), synth.vendor.as_str(), synth.params, synth.params_total), ("Big Synth", "Someone", 1, 900));
        assert_eq!(synth.path, PathBuf::from("/no/such/Synth.vst3"));
        assert_eq!(rig.plugins.folders(), [PathBuf::from("/plugins")]);

        rig.plugins.scan().unwrap();
        assert!(rig.plugins.installed().is_empty(), "an empty scan replaces the list whole");
    }

    #[test]
    fn no_cache_means_no_plugins_and_a_broken_cache_is_logged() {
        let rig = rig(Vec::new());
        rig.plugins.load_cached();
        assert!(rig.effects.kinds().is_empty() && rig.instruments.kinds().is_empty() && rig.log.recent(10).is_empty());
        rig.cache.save("{ not json").unwrap();
        rig.plugins.load_cached();
        assert!(rig.effects.kinds().is_empty() && rig.instruments.kinds().is_empty());
        assert_eq!(rig.log.recent(10).len(), 1);
    }

    #[test]
    fn scan_replaces_both_groups_remembers_the_result_and_reports_failures() {
        let root = std::env::temp_dir().join(format!("daw-vst3-scan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Broken.vst3/Contents")).unwrap();
        let rig = rig(vec![root.clone()]);
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        assert_eq!((rig.effects.kinds().len(), rig.instruments.kinds().len()), (1, 1));

        let report = rig.plugins.scan().unwrap();
        std::fs::remove_dir_all(&root).ok();
        assert_eq!((report.found, report.effects, report.instruments), (0, 0, 0));
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].path.contains("Broken.vst3"));
        assert!(
            rig.effects.kinds().is_empty() && rig.instruments.kinds().is_empty(),
            "the previous set is replaced by what was found (nothing)"
        );
        assert!(rig.cache.load().unwrap().contains("\"plugins\": []"), "the result is remembered");
        assert_eq!(rig.log.recent(10).len(), 1, "the plugin failure is logged too");
    }

    #[test]
    fn a_disabled_plugin_is_hidden_refused_with_a_clear_reason_and_still_listed() {
        let rig = rig(Vec::new());
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        rig.plugins.set_disabled("vst3:fuzz_box", true).unwrap();
        assert_eq!(rig.prefs.get().disabled_plugins, ["vst3:fuzz_box"], "the list is remembered in preferences");
        {
            let effects = rig.effects.0.lock().unwrap();
            assert!(!effects[0].listed(), "not offered in the catalog");
            let error = effects[0].create(&Params::new(), None).err().expect("a disabled plugin is not created");
            assert!(error.is(plugins::DISABLED) && error.params().contains(&("name", "Fuzz Box".into())), "{error}");
        }
        assert!(rig.log.recent(10).is_empty(), "an expected refusal: whoever created it logs it");
        let fuzz = rig.plugins.installed().into_iter().find(|p| p.kind == "vst3:fuzz_box").unwrap();
        assert!(fuzz.disabled, "marked in the installed list");

        rig.plugins.set_disabled("vst3:fuzz_box", true).unwrap();
        assert_eq!(rig.prefs.get().disabled_plugins.len(), 1, "a repeat changes nothing");
        assert!(matches!(rig.plugins.set_disabled("vst3:nothing", true), Err(e) if e.is(plugins::NOT_FOUND)));
        rig.plugins.set_disabled("vst3:nothing", false).unwrap();

        rig.plugins.set_disabled("vst3:fuzz_box", false).unwrap();
        assert!(rig.prefs.get().disabled_plugins.is_empty());
        assert!(rig.effects.0.lock().unwrap()[0].listed(), "offered again");
    }

    #[test]
    fn without_a_preferences_store_plugins_cannot_be_disabled() {
        let log: Arc<dyn AppLog> = Arc::new(Log::default());
        let plugins =
            Vst3Plugins::new(Vec::new(), Arc::new(Groups::default()), Arc::new(InstrumentSet::default()), Arc::new(Cache::default()), log);
        assert!(matches!(plugins.set_disabled("vst3:x", true), Err(e) if e.is(plugins::DISABLED_LIST_UNAVAILABLE)));
    }

    /// A folder with two bundles: "Crash" crashes the helper, "Fine" gives the effect "Fuzz Box".
    fn two_bundles(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("daw-vst3-isolated-{name}-{}", std::process::id()));
        for bundle in ["Crash.vst3", "Fine.vst3"] {
            std::fs::create_dir_all(root.join(bundle)).unwrap();
        }
        root
    }

    #[test]
    fn a_plugin_crashing_the_helper_is_reported_and_the_scan_goes_on() {
        let documents: CacheDocument = serde_json::from_str(CACHED).unwrap();
        let answer = serde_json::json!({ "outcome": { "plugins": [documents.plugins[0]], "failed": [] }, "journal": [] });
        let helper =
            fake_helper("crash-or-answer", &format!("case \"$1\" in *Crash*) kill -SEGV $$;; esac\ncat > \"$2\" <<'EOF'\n{answer}\nEOF"));
        let root = two_bundles("crash");
        let mut rig = rig(vec![root.clone()]);
        rig.plugins = rig.plugins.with_scan_helper(helper);

        let report = rig.plugins.scan().unwrap();
        std::fs::remove_dir_all(&root).ok();
        assert_eq!((report.found, report.effects), (1, 1), "the second bundle is found despite the first one crashing");
        assert_eq!(rig.effects.kinds(), ["vst3:fuzz_box"]);
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].path.ends_with("Crash.vst3"));
        assert!(report.failed[0].reason.contains("crashed while loading (signal 11"), "{}", report.failed[0].reason);
        assert_eq!(rig.log.recent(10).len(), 1, "the crash is logged too");
    }

    #[test]
    fn a_missing_helper_is_a_clear_error_and_changes_nothing() {
        let mut rig = rig(Vec::new());
        rig.cache.save(CACHED).unwrap();
        rig.plugins.load_cached();
        rig.plugins = rig.plugins.with_scan_helper(PathBuf::from("/no/such/daw-plugin-scan"));
        let error = rig.plugins.scan().unwrap_err();
        assert!(error.is(plugins::SCAN_HELPER_MISSING), "{error}");
        assert_eq!(rig.effects.kinds(), ["vst3:fuzz_box"], "the previous set is in place");
    }
}
