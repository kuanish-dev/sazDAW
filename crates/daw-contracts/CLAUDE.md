# `daw-contracts`

Project model (data, serde) and all traits: control, instruments, effects, samples, export, MIDI.

Depends on: —.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [audio-clip-editing](../../docs/features/audio-clip-editing.md), [audio-export](../../docs/features/audio-export.md), [automation](../../docs/features/automation.md), [bounce](../../docs/features/bounce.md), [channel-utility](../../docs/features/channel-utility.md), [cue](../../docs/features/cue.md), [errors](../../docs/features/errors.md), [markers-colors-folders](../../docs/features/markers-colors-folders.md), [master-meter](../../docs/features/master-meter.md), [master-visualizer](../../docs/features/master-visualizer.md), [midi-controllers](../../docs/features/midi-controllers.md), [mix-check](../../docs/features/mix-check.md), [note-edits](../../docs/features/note-edits.md), [pdc](../../docs/features/pdc.md), [preferences](../../docs/features/preferences.md), [processing-graph](../../docs/features/processing-graph.md), [recording](../../docs/features/recording.md), [sends](../../docs/features/sends.md), [song-loop](../../docs/features/song-loop.md), [templates-and-presets](../../docs/features/templates-and-presets.md), [tempo-map](../../docs/features/tempo-map.md), [virtual-stage](../../docs/features/virtual-stage.md), [warping](../../docs/features/warping.md).

## Languages of backend labels (`daw_contracts::i18n`)

English is the default, Russian the second language; the choice is `Preferences::language`. The current language is one process-wide value (`i18n::current()` / `set_current()`; why a global is explained in `i18n.rs`). Only the preferences store sets it — `PreferencesFile` on open (which `daw-bootstrap` does before building anything that makes labels) and after a successful `set`; the audio thread never reads it. Nothing caches label text: after `preferences.set` the interface re-reads the catalog and gets it in the new language. Texts already built (undo labels, names of created tracks) stay in the language they were made in.

Texts live next to the code that uses them, as an English + Russian pair (no key catalogs):
* a fixed `&'static` label — `L10n { en, ru }` (`L10n::same` when there is nothing to translate). Catalog tables use it (`ParamInfo`, `KindInfo`, factory labels, pattern names in `daw-patterns/src/data.rs`). VST3 labels come from the plugin in its own language (`daw-vst3/src/text.rs::leak_label`);
* a formatted text — `tr!("…", "…")` → `String`; undo labels are written this way; a reused label is a `const LABEL: L10n` passed as `LABEL.get()`.

Errors are not localized this way: they are registered codes translated by the interface (see "Errors as codes" in [errors](../../docs/features/errors.md)); log texts stay in English.

**Tests:** ordinary tests expect English labels; a test that switches the language lives in its own test program (`tests/language.rs` in `daw-contracts`, `daw-persistence`, `daw-bootstrap`) — a separate process, so the global switch cannot race with other tests.

**Adding a label:** write the pair at the place of use; a new catalog table entry takes `L10n`, never a bare `&str`.
