//! Protection against plugin crashes: "know which plugin the previous app session crashed in".
//!
//! A third-party plugin runs in our process (on macOS a plugin window cannot be moved to another process), so its
//! crash takes the app down. That cannot be prevented, but on the next launch we can tell who is to blame and offer to
//! disable the plugin (`PluginScanner::set_disabled`).

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The plugin inside whose audio processing the previous session ended. JSON: `{ kind, name, project, places }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginCrash {
    /// Catalog kind (`vst3:surge_xt`): the plugin is disabled by it.
    pub kind: String,
    /// Human-readable plugin name.
    pub name: String,
    /// The project opened last (from the recent list); `null` means unknown.
    pub project: Option<PathBuf>,
    /// Where this plugin sits in that project: `track "Bass"`, `effect "Distortion"`. Empty if the project is unknown
    /// or could not be read.
    pub places: Vec<String>,
}

/// "Know whether the previous session ended inside a plugin."
///
/// Contract:
/// * `last_crash` is the plugin whose audio processing the audio thread was in when the previous session ended without
///   a normal exit (`end_session` was not called). A normally closed session and a crash outside plugins give `None`.
///   The answer stays the same for the whole run until `dismiss` is called;
/// * `dismiss`: the user has seen the report; from then on `last_crash` returns `None`;
/// * `end_session` marks a normal exit: the app roots call it as the very last thing, otherwise the next launch may
///   take the exit for a crash;
/// * the report is a guess based on the "who is processing now" mark: a crash on another thread at the same moment
///   (the plugin window, our code) will be blamed on the plugin. Crashes inside the plugin window and while loading
///   it into the project are not caught.
pub trait PluginCrashGuard: Send + Sync {
    fn last_crash(&self) -> Option<PluginCrash>;
    fn dismiss(&self);
    fn end_session(&self);
}
