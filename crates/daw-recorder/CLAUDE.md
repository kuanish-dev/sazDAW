# `daw-recorder`

Recording notes played live into a track clip (audio takes are recorded by `daw-audio-recorder`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md), [recording](../../docs/features/recording.md), [song-loop](../../docs/features/song-loop.md).

## Code layout

The note recorder (`daw-recorder`: `lib.rs` — polling and the take, `pairing.rs` — press–release pairs, `take.rs` — each track's clip and loop passes, `shape.rs` — note shape) logs lost records (`source: "record"`, one entry per take; "clip deleted" is normal and not logged); a new clip starts at the beginning of a bar of the current time signature.
