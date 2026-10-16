//! VST3 effect and instrument host. **The only crate in the project where `unsafe` is allowed**: VST3 C interfaces cannot be called otherwise.
//!
//! What is inside (each entity has one job):
//! * `bundle` — a loaded `.vst3` bundle and its class factory;
//! * `instance` — a live plugin instance (component + controller), its start and stop; inside are `layout` (bus layout
//!   of an effect / instrument), `params` (the parameter list in real units, limit `MAX_EXPOSED_PARAMS`) and `display`
//!   (which plugin numbers to treat as real: value texts);
//! * `host`, `host_changes`, `host_events` — what the plugin sees from outside: `IHostApplication`, `IComponentHandler`,
//!   `IParameterChanges`, `IEventList` (notes);
//! * `midi_map` — which plugin parameter listens to each MIDI controller (`IMidiMapping`; read by `instance/midi.rs`);
//! * `runner` — runs blocks through the plugin without allocations; `convert`, `pending`, `notes`, `context` are its pure parts
//!   (fixed-capacity queues of parameter changes and notes, transport time for the plugin — `PluginClock`);
//!   `faults` — audio path failure counters and logging them;
//! * `stream` — an in-memory byte stream (`IBStream`) through which the plugin gives and takes its state;
//! * `scale` — converts parameter values "real units <-> normalized 0..1" (a per-instance table, no allocations);
//! * `param_name` — parameter names `p<id>`; `text` — VST3 strings and `'static` strings for contracts; `journal` — writing
//!   to the app log (source "vst3");
//! * `effect` — `Vst3Effect`, a plugin as `daw_contracts::Effect`; `instrument` — `Vst3Instrument`, a plugin as `Instrument`;
//! * `source` — lazy bundle loading and instance creation, shared by factories and windows;
//! * `catalog` — a plugin as a catalog effect / instrument kind (`EffectFactory`, `InstrumentFactory`), kind names
//!   and the factory set of one publication;
//! * `scanner` — `Vst3Scanner`, finds effects and instruments in folders; `isolated` — the same scan
//!   one bundle at a time in a separate process (the `daw-plugin-scan` helper, `src/bin/`), so a plugin crashing on load
//!   does not bring the app down;
//! * `guard` — `Vst3CrashGuard` (`PluginCrashGuard`): a "which plugin is processing now" marker in a memory-mapped
//!   file and the report of the previous session's crash;
//! * `plugins` — `Vst3Plugins` (`PluginScanner`): scan on demand, the `plugins.json` cache, replacing the VST3 catalog group;
//! * `directory` — the "catalog kind -> plugin" directory; `editors` — `Vst3Editors` (`PluginEditors`), plugin windows:
//!   `editor` (one open plugin interface in a separate instance), `editor_host` (the window's `IComponentHandler` and
//!   `IPlugFrame`), `project_params` (the window's target in the project: plugin kind and its params), `param_bridge` (pure
//!   "window knobs <-> project" logic: write rate, undo group),
//!   `editor_state` (pure logic of "when to write the window state to the project and when to take it from the project"),
//!   `keys` (computer keys in an instrument window play notes: `KeyPlayer` and the AppKit monitor);
//! * `test_fakes` — fake log, project and history for unit tests.
//!
//! From outside only [`Vst3Plugins`] is needed; the rest is visible only for tests and documentation.

mod bundle;
mod catalog;
mod context;
mod convert;
mod directory;
mod editor;
mod editor_host;
mod editor_state;
mod editors;
mod effect;
mod faults;
mod guard;
mod host;
mod host_changes;
mod host_events;
mod instance;
mod instrument;
mod isolated;
mod journal;
mod keys;
mod midi_map;
mod notes;
mod param_bridge;
mod param_name;
mod pending;
mod plugins;
mod project_params;
mod runner;
mod scale;
mod scanner;
mod source;
mod stream;
#[cfg(test)]
mod test_fakes;
mod text;

pub use directory::PluginDirectory;
pub use editors::Vst3Editors;
pub use guard::{ProcessMarker, Vst3CrashGuard};
pub use instance::{MAX_EXPOSED_PARAMS, ParamDesc, ParamShape};
pub use isolated::{HELPER_NAME, IsolatedScan, SCAN_TIMEOUT, helper_main};
pub use plugins::Vst3Plugins;
pub use scanner::{PluginInfo, PluginKind, ScanOutcome, Vst3Scanner};
