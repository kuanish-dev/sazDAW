//! User preferences (the "Settings" page): profile, autosave, appearance. The store's role is to "remember the
//! user's preferences" between launches (`PreferencesStore`); the rules for valid values live here so every
//! implementation (and the interface mock) checks them the same way.

use std::path::PathBuf;
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::codes::settings as codes;
use crate::{ContractError, Language};

/// Autosave every 30 seconds by default.
pub const DEFAULT_AUTOSAVE_SECONDS: u32 = 30;
/// Allowed autosave interval in seconds (besides 0, which turns autosave off).
pub const MIN_AUTOSAVE_SECONDS: u32 = 10;
pub const MAX_AUTOSAVE_SECONDS: u32 = 600;
/// Interface scale.
pub const MIN_UI_SCALE: f64 = 0.8;
pub const MAX_UI_SCALE: f64 = 1.5;
/// Author name length in characters (after trimming surrounding spaces).
pub const MAX_AUTHOR_CHARS: usize = 100;

/// All user preferences. JSON: `{ language, profile, autosaveSeconds, appearance, disabledPlugins }`.
/// Missing fields take their defaults (`Default`), so a partial file does not lose the rest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// Language of the interface and of the labels the backend builds (`crate::i18n`); `"en"` by default.
    pub language: Language,
    pub profile: Profile,
    /// How often to autosave a project with unsaved edits, in seconds: 0 is off, otherwise 10..=600.
    /// Defaults to 30.
    pub autosave_seconds: u32,
    pub appearance: Appearance,
    /// Kinds of disabled plugins (`vst3:surge_xt`), empty by default. Changed by `PluginScanner::set_disabled`
    /// (which also removes the plugin from the catalog); editing it here takes effect only after a restart.
    pub disabled_plugins: Vec<String>,
}

/// Who is working and where projects go by default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    /// Author, written e.g. into exported MIDI files. An empty string means not set. Up to 100 characters; surrounding
    /// spaces are trimmed.
    pub author: String,
    /// Folder for new projects (a full path); `null` means the default folder.
    pub projects_folder: Option<PathBuf>,
    /// Open the last project on launch. On by default: that is how the app behaved before this setting existed.
    pub open_last_project: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self { author: String::new(), projects_folder: None, open_last_project: true }
    }
}

/// Interface appearance: the backend only stores it, the interface applies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Appearance {
    pub theme: Theme,
    pub accent: Accent,
    /// Interface scale, 0.8..=1.5; 1.0 by default.
    pub ui_scale: f64,
    pub track_height: TrackHeight,
    /// Show hints. On by default.
    pub show_hints: bool,
}

/// Theme: follow the system (default), dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

/// Accent color; blue by default (the VS Code-like look).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Accent {
    Orange,
    #[default]
    Blue,
    Green,
    Purple,
    Red,
}

/// Track row height; normal by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TrackHeight {
    Compact,
    #[default]
    Normal,
    Tall,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::En,
            profile: Profile::default(),
            autosave_seconds: DEFAULT_AUTOSAVE_SECONDS,
            appearance: Appearance::default(),
            disabled_plugins: Vec::new(),
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self { theme: Theme::System, accent: Accent::default(), ui_scale: 1.0, track_height: TrackHeight::Normal, show_hints: true }
    }
}

impl Preferences {
    /// Normalizes values to their stored form (author trimmed, empty path becomes `null`) and checks the limits.
    /// Fails with the code of the first wrong field (`codes::settings`).
    pub fn checked(mut self) -> Result<Self, ContractError> {
        self.profile.author = self.profile.author.trim().to_string();
        if self.profile.author.chars().count() > MAX_AUTHOR_CHARS {
            return Err(codes::AUTHOR_TOO_LONG.with("max", MAX_AUTHOR_CHARS));
        }
        self.profile.projects_folder = self.profile.projects_folder.filter(|path| !path.as_os_str().is_empty());
        if let Some(folder) = &self.profile.projects_folder
            && !folder.is_absolute()
        {
            return Err(codes::PROJECTS_FOLDER_NOT_ABSOLUTE.with("path", folder.display()));
        }
        let seconds = self.autosave_seconds;
        if seconds != 0 && !(MIN_AUTOSAVE_SECONDS..=MAX_AUTOSAVE_SECONDS).contains(&seconds) {
            return Err(codes::AUTOSAVE_OUT_OF_RANGE
                .with("seconds", seconds)
                .with("min", MIN_AUTOSAVE_SECONDS)
                .with("max", MAX_AUTOSAVE_SECONDS));
        }
        let scale = self.appearance.ui_scale;
        if !(MIN_UI_SCALE..=MAX_UI_SCALE).contains(&scale) {
            return Err(codes::UI_SCALE_OUT_OF_RANGE.with("scale", scale).with("min", MIN_UI_SCALE).with("max", MAX_UI_SCALE));
        }
        Ok(self)
    }

    /// Autosave interval; `None` means off.
    pub fn autosave_interval(&self) -> Option<Duration> {
        (self.autosave_seconds > 0).then(|| Duration::from_secs(u64::from(self.autosave_seconds)))
    }
}

/// Remembers the user's preferences between launches.
///
/// Contract:
/// * `get` always returns valid preferences: nothing saved means defaults; saved preferences that are corrupted or
///   out of range also give defaults, the reason is written to the log (`AppLog`) and the app keeps working;
/// * `set` checks and normalizes the values (`Preferences::checked`): wrong values fail with an `ErrorKind::Invalid`
///   or `ErrorKind::Limit` code from `codes::settings` and leave the saved ones unchanged; valid ones are written
///   atomically (a failure mid-write does not corrupt the previous ones) and returned as saved; a write failure is an
///   `ErrorKind::System` error, and `get` still returns the previous preferences;
/// * changes are visible at once: whoever reads `get` (autosave, MIDI export) gets the new values without a restart;
/// * the language is process-wide: the store makes `language` current (`i18n::set_current`) when it is created (the
///   saved value, or the default) and after every successful `set`; a rejected `set` leaves the language as it was.
pub trait PreferencesStore: Send + Sync {
    fn get(&self) -> Preferences;
    fn set(&self, preferences: Preferences) -> Result<Preferences, ContractError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn defaults_and_wire_shape_match_the_agreed_json() {
        let json = serde_json::to_value(Preferences::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "language": "en",
                "profile": { "author": "", "projectsFolder": null, "openLastProject": true },
                "autosaveSeconds": 30,
                "appearance": { "theme": "system", "accent": "blue", "uiScale": 1.0, "trackHeight": "normal", "showHints": true },
                "disabledPlugins": []
            })
        );
        assert_eq!(Preferences::default().autosave_interval(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn language_roundtrips_defaults_to_english_and_rejects_unknown_values() {
        let parsed: Preferences = serde_json::from_str(r#"{ "language": "ru" }"#).unwrap();
        assert_eq!(parsed.language, Language::Ru);
        assert_eq!(serde_json::to_value(&parsed).unwrap()["language"], "ru");
        let missing: Preferences = serde_json::from_str("{}").unwrap();
        assert_eq!(missing.language, Language::En);
        assert!(serde_json::from_str::<Preferences>(r#"{ "language": "de" }"#).is_err());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let parsed: Preferences = serde_json::from_str(r#"{ "appearance": { "theme": "dark" } }"#).unwrap();
        assert_eq!(parsed.appearance.theme, Theme::Dark);
        assert_eq!(parsed.appearance.ui_scale, 1.0);
        assert_eq!(parsed.autosave_seconds, DEFAULT_AUTOSAVE_SECONDS);
    }

    #[test]
    fn checked_normalizes_author_and_empty_folder() {
        let mut prefs = Preferences::default();
        prefs.profile.author = "  Anya  ".into();
        prefs.profile.projects_folder = Some(PathBuf::new());
        let checked = prefs.checked().unwrap();
        assert_eq!(checked.profile.author, "Anya");
        assert_eq!(checked.profile.projects_folder, None);
    }

    #[test]
    fn checked_rejects_values_out_of_range() {
        let with = |change: fn(&mut Preferences)| {
            let mut prefs = Preferences::default();
            change(&mut prefs);
            prefs.checked()
        };
        assert!(with(|p| p.autosave_seconds = 0).unwrap().autosave_interval().is_none(), "0 means off");
        assert!(with(|p| p.autosave_seconds = 10).is_ok() && with(|p| p.autosave_seconds = 600).is_ok());
        for bad in [
            with(|p| p.autosave_seconds = 5),
            with(|p| p.autosave_seconds = 601),
            with(|p| p.appearance.ui_scale = 0.7),
            with(|p| p.appearance.ui_scale = 1.6),
            with(|p| p.appearance.ui_scale = f64::NAN),
            with(|p| p.profile.author = "é".repeat(101)),
            with(|p| p.profile.projects_folder = Some("relative/dir".into())),
        ] {
            assert!(matches!(&bad, Err(e) if e.kind() == ErrorKind::Invalid || e.kind() == ErrorKind::Limit), "{bad:?}");
        }
        assert!(with(|p| p.profile.author = format!(" {} ", "é".repeat(100))).is_ok(), "limit is after trim");
    }
}
