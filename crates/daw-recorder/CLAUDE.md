# `daw-recorder`

Recording notes played live into a track clip (audio takes are recorded by `daw-audio-recorder`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md), [recording](../../docs/features/recording.md), [song-loop](../../docs/features/song-loop.md).

## Code layout

`lib.rs` — polling and the take (lost records are logged, `source: "record"`), `pairing.rs` — press–release pairs, `take.rs` — each track's clip and loop passes, `shape.rs` — note shape, `controllers.rs` — pitch bend, CC and pressure into the clip's controller lanes.
