# `daw-midi`

MIDI files (midly: notes and controller lanes; on export the author from preferences goes into the Copyright event) and live input (midir: notes and controllers; a CC goes to MIDI learn mappings first). Value conversion shared by both — `values.rs`. See "MIDI controllers" ([docs/features/midi-controllers.md](../../docs/features/midi-controllers.md)).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md), [tempo-map](../../docs/features/tempo-map.md).
