//! Storage on disk: the project folder (`<name>.dawproj` + `samples/` + `autosave/` + `exports/`), copying sounds into
//! the project, autosave, the error log and the small app state files (recent projects, linked folders, audio device
//! choice, metronome settings, plugin list, user preferences), instrument and effect presets (`presets.rs`) and project templates
//! (`templates.rs`). Every write is atomic (`atomic.rs`).

mod assets;
mod atomic;
mod audio;
mod autosave;
mod json_state;
mod layout;
mod log;
mod metronome;
mod plugin_cache;
mod preferences;
mod presets;
mod recent;
mod sources;
mod storage;
mod templates;
#[cfg(test)]
mod testing;

pub use assets::FsAssets;
pub use audio::AudioSettingsFile;
pub use autosave::{AutosaveInterval, Autosaver, ProjectAutosave};
pub use log::FileLog;
pub use metronome::MetronomeSettingsFile;
pub use plugin_cache::PluginCacheFile;
pub use preferences::PreferencesFile;
pub use presets::{PresetFolder, PresetTargets};
pub use recent::RecentStore;
pub use sources::SourceStore;
pub use storage::JsonStorage;
pub use templates::TemplateFolder;
