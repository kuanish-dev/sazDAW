# `daw-bootstrap`

**Session assembly**: the only place where implementations are chosen. `lib.rs` — `Session`, state folders, `build_session*`; wiring by area — `wiring/` (`catalogs.rs`, `project.rs`, `audio.rs`, `drums.rs`, the overall order and `Controls` — `mod.rs`); background services — `services.rs` (`start_autosave()` takes the interval from user preferences). Integration tests of the full session — `tests/` (`session.rs`, `render_*.rs` with shared helpers in `tests/common/`).

Depends on: everything.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [audio-export](../../docs/features/audio-export.md), [bounce](../../docs/features/bounce.md), [errors](../../docs/features/errors.md), [master-meter](../../docs/features/master-meter.md), [midi-controllers](../../docs/features/midi-controllers.md), [mix-check](../../docs/features/mix-check.md), [preferences](../../docs/features/preferences.md), [recording](../../docs/features/recording.md), [song-loop](../../docs/features/song-loop.md), [templates-and-presets](../../docs/features/templates-and-presets.md), [tempo-map](../../docs/features/tempo-map.md), [warping](../../docs/features/warping.md).
