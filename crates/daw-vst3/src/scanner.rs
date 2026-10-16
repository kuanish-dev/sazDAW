//! `Vst3Scanner`: find VST3 effects and instruments installed on the system.
//!
//! Looks for `*.vst3` bundles in the given folders (up to two levels deep), loads each, lists
//! "Audio Module Class" classes with subcategory "Fx" (effect) or "Instrument" (instrument), creates an instance for the scan to
//! read the parameters, and releases everything. A plugin that fails to load goes to the log and to the result's `failed`, and is skipped.
//! It loads plugins in ITS OWN process: the app scans third-party plugins through the helper (`isolated.rs`), which
//! calls `scan_bundle` in a separate process; here, in the app process, only tests (with the dev-only fixture plugins).

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_contracts::{AppLog, ScanFailure};
use serde::{Deserialize, Serialize};
use vst3::Steinberg::{IPluginFactory2, IPluginFactory2Trait, IPluginFactoryTrait, PClassInfo2, PFactoryInfo, kResultOk};

use crate::bundle::Bundle;
use crate::instance::{ClassId, ParamDesc, PluginInstance};
use crate::journal;
use crate::text::from_c_chars;

const AUDIO_CLASS: &str = "Audio Module Class";
const EFFECT_SUBCATEGORY: &str = "Fx";
const INSTRUMENT_SUBCATEGORY: &str = "Instrument";
const MAX_DEPTH: usize = 2;

/// What the plugin is to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginKind {
    Effect,
    Instrument,
}

/// Description of a found plugin (effect or instrument).
/// Everything a factory needs to show the plugin in the catalog without loading it; kept in the cache (`plugins.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub path: PathBuf,
    pub class_id: ClassId,
    pub name: String,
    pub vendor: String,
    pub kind: PluginKind,
    /// The first suitable plugin parameters, at most `MAX_EXPOSED_PARAMS`.
    pub params: Vec<ParamDesc>,
    /// How many suitable parameters the plugin has in total (more than `params.len()` if the limit kicked in).
    pub params_total: usize,
}

/// Result of looking through folders: found plugins and those that did not load.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanOutcome {
    pub plugins: Vec<PluginInfo>,
    pub failed: Vec<ScanFailure>,
}

impl ScanOutcome {
    pub fn merge(&mut self, other: ScanOutcome) {
        self.plugins.extend(other.plugins);
        self.failed.extend(other.failed);
    }
}

pub struct Vst3Scanner {
    dirs: Vec<PathBuf>,
    log: Arc<dyn AppLog>,
}

/// `*.vst3` bundles in `dir` and nested folders. A missing folder is normal (the user has no plugins of their own);
/// any other read error goes to the log: otherwise plugins would vanish from the scan without explanation.
fn collect_bundles(dir: &Path, depth: usize, found: &mut Vec<PathBuf>, log: &dyn AppLog) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => return journal::warn(log, format!("VST3: could not read folder {}: {error}", dir.display())),
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.extension().is_some_and(|ext| ext == "vst3") {
            found.push(path);
        } else if depth < MAX_DEPTH && path.is_dir() {
            collect_bundles(&path, depth + 1, found, log);
        }
    }
}

impl Vst3Scanner {
    pub fn new(dirs: Vec<PathBuf>, log: Arc<dyn AppLog>) -> Self {
        Self { dirs, log }
    }

    /// All bundles in the scan folders, ordered by name.
    pub fn bundles(&self) -> Vec<PathBuf> {
        let mut bundles = Vec::new();
        for dir in &self.dirs {
            collect_bundles(dir, 0, &mut bundles, &*self.log);
        }
        bundles
    }

    /// Loads all bundles in this process (tests only; the app loads third-party bundles only through `isolated.rs`).
    pub fn scan(&self) -> ScanOutcome {
        let mut outcome = ScanOutcome::default();
        for path in &self.bundles() {
            outcome.merge(self.scan_bundle(path));
        }
        outcome
    }

    /// A plugin failure is visible to the user (in the scan result) and to the owner (in the log).
    fn fail(&self, outcome: &mut ScanOutcome, path: &Path, reason: String) {
        journal::warn(&*self.log, format!("VST3 {}: {reason}", path.display()));
        outcome.failed.push(ScanFailure { path: path.display().to_string(), reason });
    }

    /// Loads one bundle in this process.
    pub fn scan_bundle(&self, path: &Path) -> ScanOutcome {
        let mut outcome = ScanOutcome::default();
        let bundle = match Bundle::load(path) {
            Ok(bundle) => Arc::new(bundle),
            Err(error) => {
                self.fail(&mut outcome, path, error);
                return outcome;
            }
        };
        let factory_vendor = factory_vendor(&bundle);
        // SAFETY: a plain factory call without pointers.
        let count = unsafe { bundle.factory().countClasses() };
        let Some(factory2) = bundle.factory().cast::<IPluginFactory2>() else {
            self.fail(&mut outcome, path, "factory without IPluginFactory2, class types are unknown".into());
            return outcome;
        };
        for index in 0..count {
            // SAFETY: a struct of numbers and arrays, the zero value is valid.
            let mut info: PClassInfo2 = unsafe { std::mem::zeroed() };
            // SAFETY: `info` is a local struct the factory writes the class description into.
            let result = unsafe { factory2.getClassInfo2(index, &raw mut info) };
            if result != kResultOk {
                journal::warn(&*self.log, format!("VST3 {}: class #{index} skipped, getClassInfo2 returned code {result}", path.display()));
                continue;
            }
            if from_c_chars(&info.category) != AUDIO_CLASS {
                continue;
            }
            let Some(kind) = plugin_kind(&from_c_chars(&info.subCategories)) else {
                continue;
            };
            let vendor = Some(from_c_chars(&info.vendor)).filter(|v| !v.is_empty()).unwrap_or_else(|| factory_vendor.clone());
            let class_id: ClassId = info.cid.map(|c| c as u8);
            let name = from_c_chars(&info.name);
            match PluginInstance::create(bundle.clone(), &class_id) {
                Ok(instance) => {
                    let list = instance.parameters();
                    outcome.plugins.push(PluginInfo {
                        path: path.to_path_buf(),
                        class_id,
                        name,
                        vendor,
                        kind,
                        params: list.params,
                        params_total: list.total,
                    });
                }
                Err(error) => self.fail(&mut outcome, path, format!("«{name}»: {error}")),
            }
        }
        outcome
    }
}

/// `Fx` among the subcategories (`Fx|Distortion`) is an effect, `Instrument` (`Instrument|Synth`) an instrument; the rest (analyzers…) is skipped.
/// If both are given, the plugin counts as an effect: an instrument with an audio input is essentially an effect.
fn plugin_kind(sub_categories: &str) -> Option<PluginKind> {
    let has = |wanted: &str| sub_categories.split('|').any(|part| part.trim() == wanted);
    if has(EFFECT_SUBCATEGORY) {
        Some(PluginKind::Effect)
    } else if has(INSTRUMENT_SUBCATEGORY) {
        Some(PluginKind::Instrument)
    } else {
        None
    }
}

fn factory_vendor(bundle: &Bundle) -> String {
    // SAFETY: a struct of numbers and arrays, the zero value is valid.
    let mut info: PFactoryInfo = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a local struct the factory writes its own details into.
    if unsafe { bundle.factory().getFactoryInfo(&raw mut info) } == kResultOk { from_c_chars(&info.vendor) } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcategories_are_detected_exactly() {
        assert_eq!(plugin_kind("Fx|Distortion"), Some(PluginKind::Effect));
        assert_eq!(plugin_kind("Fx"), Some(PluginKind::Effect));
        assert_eq!(plugin_kind("Instrument|Synth"), Some(PluginKind::Instrument));
        assert_eq!(plugin_kind("Instrument"), Some(PluginKind::Instrument));
        assert_eq!(plugin_kind("Fx|Instrument"), Some(PluginKind::Effect));
        assert_eq!(plugin_kind("Analyzer|FxLike"), None);
        assert_eq!(plugin_kind("InstrumentLike"), None);
        assert_eq!(plugin_kind(""), None);
    }
}
