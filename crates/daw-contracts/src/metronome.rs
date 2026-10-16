//! Metronome (`MetronomeControl`) and where its settings are remembered between launches (`MetronomeSettingsStore`).

/// The metronome's settings, remembered between launches. Off by default: a click nobody asked for is a surprise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetronomeSettings {
    pub enabled: bool,
}

/// The metronome switch. Job: turn the beat click on and off from any thread.
///
/// Contract: it starts as the settings were left in the last launch (`MetronomeSettingsStore::load`; off on the
/// first launch), and every change is remembered at once (`MetronomeSettingsStore::save`). A failed save does not
/// undo the switch: the click follows the user now, and the failure goes to the log.
pub trait MetronomeControl: Send + Sync {
    fn set_enabled(&self, enabled: bool);
    fn is_enabled(&self) -> bool;
}

/// Job: remember the metronome settings between launches.
pub trait MetronomeSettingsStore: Send + Sync {
    /// The stored settings; a missing or damaged file gives the defaults (a damaged one is logged).
    fn load(&self) -> MetronomeSettings;
    fn save(&self, settings: &MetronomeSettings) -> Result<(), crate::ContractError>;
}
