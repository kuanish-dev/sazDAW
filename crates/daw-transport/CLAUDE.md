# `daw-transport`

Clock and transport control (atomics; loop start and end live in one atomic and are read together), loop with a frame-accurate wrap, the song loop (back to the start at the song end, see "Song loop" ([docs/features/song-loop.md](../../docs/features/song-loop.md))), the cue (start position `play`/`stop` return to); the clock takes the tempo from the tempo map that the engine passes into every `advance` (see "Tempo map" ([docs/features/tempo-map.md](../../docs/features/tempo-map.md))).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [cue](../../docs/features/cue.md), [song-loop](../../docs/features/song-loop.md).
