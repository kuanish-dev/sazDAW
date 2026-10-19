# `daw-app`

Console root: audio output, autosave, commands.

Depends on: contracts, bootstrap.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [tempo-map](../../docs/features/tempo-map.md).

## Console commands

The console is `src/commands/`: `mod.rs` (word parsing `Words`, `HELP`, the dispatcher, `report`/`checked` for errors), one file per area (`transport`, `tracks`, `clips`, `graph`, `project`, `library`), each returning `true` if the command is its own. A new command is a line in the right area and in `HELP`.

The windowless `daw-mcp` binary is `src/mcp_main.rs` (see "AI interface" in [daw-rpc](../daw-rpc/CLAUDE.md)).
