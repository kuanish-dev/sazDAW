# `src/api/` — the communication contract and the model

`src/api/`: `rpc.ts` (`RpcClient`), `dialogs.ts` (`SystemDialogs`), `shell.ts` (`AppShell`: window zoom and app version), `types/` (a mirror of the Rust JSON, split by area: `model`, `graph`, `stage`, `transport`, `project`, `catalog`, `samples`, `audio`, `drums`, `preferences`; imported via `api/types`), `api.ts` — typed wrappers; the groups match the RPC namespaces (`samples.*`, `history.*`, `metronome.*`, `patterns.*`…)

Knows about: —.

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md). Cross-layer features it takes part in: [audio-export](../../../docs/features/audio-export.md), [automation](../../../docs/features/automation.md), [cue](../../../docs/features/cue.md), [master-meter](../../../docs/features/master-meter.md), [midi-controllers](../../../docs/features/midi-controllers.md), [preferences](../../../docs/features/preferences.md), [recording](../../../docs/features/recording.md), [sounds](../../../docs/features/sounds.md), [virtual-stage](../../../docs/features/virtual-stage.md).
