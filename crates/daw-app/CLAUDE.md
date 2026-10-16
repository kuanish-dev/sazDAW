# `daw-app`

Console root: audio output, autosave, commands.

Depends on: contracts, bootstrap.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [tempo-map](../../docs/features/tempo-map.md).

## Console commands

The `daw-app` console is `src/commands/`: `mod.rs` (word parsing `Words`, help, dispatcher), one file per area (`transport`, `tracks`, `clips`, `graph`, `project`, `library`), each returns `true` if the command is its own; a new command is a line in the right area and in `HELP`; errors are printed by `report`/`checked`; `mute`/`solo` work on tracks and buses.
