//! Effects: the realtime side (`Effect`), per-kind factories, the catalog and third-party plugin scanning.

use std::path::PathBuf;

use schemars::JsonSchema;

use crate::{BlockTiming, ContractError, KindInfo, L10n, Params, PluginState, StreamConfig};

/// The effect of a graph node. It is created on the control side (where `prepare` is also called); once handed to
/// the engine, every method is called from the realtime thread on a stereo buffer: no allocations, locks or I/O.
pub trait Effect: Send {
    fn prepare(&mut self, config: StreamConfig);
    /// Unknown names are ignored.
    fn set_param(&mut self, name: &str, value: f32);
    fn reset(&mut self);
    /// Processes an interleaved stereo buffer in place.
    /// `sidechain` is the key signal (same size), if one is connected.
    fn process(&mut self, interleaved: &mut [f32], sidechain: Option<&[f32]>);
    /// The effect's latency in frames: how far the output lags behind the input. The engine uses it to align parallel
    /// paths (plugin delay compensation, PDC). Same contract as `Instrument::latency_samples`: valid after `prepare`,
    /// may change after `prepare` or a parameter change, re-read by the engine when applying a snapshot and after
    /// `prepare`; called from the audio thread, so no allocations or locks. Effects without latency return 0.
    fn latency_samples(&self) -> u32 {
        0
    }
    /// How much the effect is turning the sound down right now, in dB (0 or more): the gain reduction a compressor, gate
    /// or limiter shows on its meter. Called from the audio thread after `process` (no allocations or locks); `None`
    /// (the default) for effects that do not reduce gain.
    fn gain_reduction_db(&self) -> Option<f32> {
        None
    }
    /// Timing of the segment the engine is about to process: called from the audio thread before `process` of every
    /// segment (no allocations or locks). Same contract as `Instrument::set_timing`; does nothing by default.
    fn set_timing(&mut self, _timing: &BlockTiming) {}
}

/// Creates effects of one kind. Unlike instruments, an effect is never rebuilt on a parameter change (there is no
/// `rebuild_on`): every effect parameter changes through `Effect::set_param`.
pub trait EffectFactory: Send + Sync {
    fn kind(&self) -> &'static str;
    fn label(&self) -> L10n;
    fn describe(&self) -> Vec<crate::ParamInfo>;
    /// A new effect. `state` is the saved plugin state from the project (`NodeKind::Effect::state`): it is applied
    /// first and `params` go on top. Kinds without state of their own (built-in ones) ignore it.
    fn create(&self, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError>;
    /// Whether the kind is offered in the catalog (`EffectCatalog::kinds`). `false` means the kind is known (a project
    /// using it opens, and `create` explains why the effect is missing) but cannot be picked: this hides a disabled
    /// plugin.
    fn listed(&self) -> bool {
        true
    }
}

/// Catalog of available effects.
pub trait EffectCatalog: Send + Sync {
    fn kinds(&self) -> Vec<KindInfo>;
    fn defaults(&self, kind: &str) -> Result<Params, ContractError>;
    /// See [`EffectFactory::create`].
    fn create(&self, kind: &str, params: &Params, state: Option<&PluginState>) -> Result<Box<dyn Effect>, ContractError>;
}

/// A mutable set of effect factories split into named groups (built-in, VST3, …).
/// This is how plugin scanning puts what it found into the catalog without knowing how the catalog works.
pub trait EffectGroups: Send + Sync {
    /// Replaces all factories of `group` with `factories` (creating the group if missing). Other groups are untouched.
    /// Effects already created by the previous factories keep working.
    fn replace_group(&self, group: &str, factories: Vec<Box<dyn EffectFactory>>);
}

/// A plugin that failed to load during a scan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct ScanFailure {
    pub path: String,
    pub reason: String,
}

/// Result of a plugin scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct ScanReport {
    /// Total plugins found (`effects + instruments`).
    pub found: usize,
    /// Of them, effects (now available in the effect catalog).
    pub effects: usize,
    /// Of them, instruments (now available in the instrument catalog).
    pub instruments: usize,
    pub failed: Vec<ScanFailure>,
}

/// Scans for third-party plugins (effects and instruments) when the user asks (the app never scans on startup: a
/// third-party plugin may crash while loading).
///
/// Contract:
/// * a new scan replaces the previous set of found plugins entirely (plugins gone from disk leave the catalog);
/// * effects already created in projects are not affected;
/// * results are remembered on disk, and on the next start the app picks them up WITHOUT loading the plugins
///   themselves (a plugin is loaded only when its first effect is created);
/// * a failure of one plugin does not stop the scan: it goes to `ScanReport::failed` and to the `AppLog`; this also
///   covers a plugin that crashes or hangs the process while loading (the implementation loads them outside the app
///   process); `Err` only when the scan could not run at all.
pub trait PluginScanner: Send + Sync {
    fn scan(&self) -> Result<ScanReport, ContractError>;
    /// Plugins currently in the catalogs (built-in ones and those found by the last scan or taken from its cache),
    /// without loading the plugins. Order: built-in first, then instruments, then effects; by name within each.
    fn installed(&self) -> Vec<InstalledPlugin>;
    /// Folders `scan` searches for plugins (empty means the scan finds nothing).
    fn folders(&self) -> Vec<PathBuf>;
    /// Disables the plugin of kind `kind` (for example after a crash) or enables it again. A disabled plugin stays in
    /// `installed` (with `disabled: true`) but is not offered in the catalogs, and creating it fails with
    /// `plugins::DISABLED`: a project using it opens with a warning, without that instrument or effect. Instances
    /// already created keep working until the project is reopened. The list is remembered between runs. Disabling a
    /// kind not in `installed` is `plugins::NOT_FOUND`; any kind can be enabled (repeating changes nothing).
    fn set_disabled(&self, kind: &str, disabled: bool) -> Result<(), ContractError>;
}

/// What a plugin is to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PluginType {
    Effect,
    Instrument,
}

/// A plugin in the catalog (for the installed plugins list).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    /// Kind in the effect or instrument catalog (`vst3:surge_xt`), used to create the plugin.
    pub kind: String,
    pub name: String,
    pub vendor: String,
    /// Location of the plugin file (bundle).
    pub path: PathBuf,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    /// Number of parameters visible to the app (capped by the implementation's limit).
    pub params: u32,
    /// Total number of suitable parameters the plugin has.
    pub params_total: u32,
    /// Disabled by the user (`PluginScanner::set_disabled`): neither offered nor created.
    pub disabled: bool,
}

/// Storage for the remembered plugin list (one text document). The document format is up to the store.
pub trait PluginCacheStore: Send + Sync {
    /// `None` when there is no list or it cannot be read (then there are no plugins until a scan runs).
    fn load(&self) -> Option<String>;
    /// Writes the whole document atomically: a failure mid-write does not corrupt the previous list.
    fn save(&self, text: &str) -> Result<(), ContractError>;
}
