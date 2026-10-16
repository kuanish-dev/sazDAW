# `daw-persistence`

Project folder (`<name>.dawproj`, `samples/`, `autosave/`, `exports/`): JSON storage (`storage.rs`; a sample that failed to copy is logged, the reference stays), copying sounds (`AssetStore`), recent projects (`RecentProjects`), autosave (`AutosaveControl`), user preferences (`PreferencesFile`), small state files. All writes are atomic — a single function `atomic.rs::write_atomic`; instrument and effect presets (`PresetFolder`, `presets.rs`) and project templates (`TemplateFolder`, `templates.rs`) live in the app state folder (see "Project templates and presets" ([docs/features/templates-and-presets.md](../../docs/features/templates-and-presets.md))).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [errors](../../docs/features/errors.md), [preferences](../../docs/features/preferences.md), [templates-and-presets](../../docs/features/templates-and-presets.md).

## Small app state

Small app state (`recent.json`, `sound-sources.json`, `preferences.json`; `audio.json` and `metronome.json` are read with the same `read_or_default`) is in `daw-persistence/src/json_state.rs` (`JsonState<T>`: reading, lock, atomic write; a corrupted file and a write failure go to the log with `source: "persist"`, the app does not crash).

## Autosave

Autosave (`daw-persistence/src/autosave.rs`) logs write failures (`source: "autosave"`), one entry per run of consecutive failures; otherwise edits would silently stop being saved (rule 11). `Autosaver` receives the interval as a function (`AutosaveInterval = Box<dyn Fn() -> Option<Duration>>`, `None` means off) and asks it on every step; the thread wakes up at least once a second (`RECHECK`), so a settings change takes effect immediately. `Session::start_autosave()` passes `preferences.get().autosave_interval()`.
