//! `PluginSource`: "give a live instance of a found plugin" — shared by effect and instrument factories and plugin windows.
//! Creating a source does NOT load the plugin: the bundle is opened lazily, on the first instance.

use std::sync::{Arc, Mutex};

use daw_contracts::sync::lock;
use daw_contracts::{L10n, ParamInfo, ParamKind};

use crate::bundle::Bundle;
use crate::instance::{ParamShape, PluginInstance};
use crate::param_name;
use crate::scanner::PluginInfo;
use crate::text::{leak, leak_label, leak_list};

pub struct PluginSource {
    info: PluginInfo,
    /// Label and params for the catalog: the `'static` strings leak once per source, and the source survives
    /// repeated scans (`Vst3Plugins::publish` takes the previous one if the plugin did not change).
    label: L10n,
    params: Vec<ParamInfo>,
    /// The loaded bundle shared by all instances. Kept until the app exits: loading and unloading plugin
    /// libraries again is riskier than a few extra megabytes.
    /// The lock is also held while an instance is created: so instances of one bundle are created one at a time
    /// (the `Send`/`Sync` of `Bundle` relies on this).
    bundle: Mutex<Option<Arc<Bundle>>>,
}

/// Plugin parameters as contract parameters: `p<id>` in the plugin's real units (conversion to normalized
/// values is the instance's `ParamScales`), choices named as the plugin names them.
fn param_infos(info: &PluginInfo) -> Vec<ParamInfo> {
    info.params
        .iter()
        .map(|p| ParamInfo {
            name: leak(param_name::name(p.id)),
            label: leak_label(p.label.clone()),
            min: p.min as f32,
            max: p.max as f32,
            default: p.default as f32,
            kind: match p.shape {
                ParamShape::Number => ParamKind::Number,
                ParamShape::Switch => ParamKind::Switch,
                ParamShape::Choice => ParamKind::Choice,
            },
            choices: leak_list(&p.choices),
        })
        .collect()
}

impl PluginSource {
    pub fn new(info: PluginInfo) -> Self {
        let label = leak_label(format!("{} (VST3)", info.name));
        let params = param_infos(&info);
        Self { info, label, params, bundle: Mutex::new(None) }
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    pub fn param_infos(&self) -> &[ParamInfo] {
        &self.params
    }

    pub fn label(&self) -> L10n {
        self.label
    }

    /// A new plugin instance (component + controller).
    pub fn instance(&self) -> Result<PluginInstance, String> {
        let mut cached = lock(&self.bundle);
        let bundle = if let Some(bundle) = cached.as_ref() {
            bundle.clone()
        } else {
            // The plugin list came from the cache: the plugin may have been removed since. Say so plainly.
            if !self.info.path.exists() {
                return Err("the plugin file is gone from disk; scan plugins again".into());
            }
            let bundle = Arc::new(Bundle::load(&self.info.path)?);
            *cached = Some(bundle.clone());
            bundle
        };
        // Under the lock (`cached` is still alive): instances of this bundle are created one at a time.
        PluginInstance::create(bundle, &self.info.class_id)
    }

    /// Error text with the plugin name and path, for the log and the user.
    pub fn describe_error(&self, error: &str) -> String {
        format!("plugin \"{}\" ({}): {error}", self.info.name, self.info.path.display())
    }
}
