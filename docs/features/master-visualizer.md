# Master visualizer

**Master visualizer** (spectrum and oscilloscope on the master strip, `mixer/MasterVisualizer.tsx`).

- Contract: `EngineStats::master_window() → MasterWindow` (`daw-contracts/src/engine.rs`, `MASTER_WINDOW_FRAMES`). No on/off switch: a read keeps the capture going for `CAPTURE_LEASE_SECONDS`, so any number of readers share it and a reader that goes away stops it by itself (see "Captures are leased by reads" in [daw-engine](../../crates/daw-engine/CLAUDE.md)).
- Engine: `daw-engine/src/scope.rs: MasterScope` in `Telemetry` — a preallocated ring of atomics that the audio thread fills in `process_segment` only while the lease runs, after the preview voice and before the ±1 clamp and the metronome (the picture shows the music, not the clicks); the control side copies it out, so the audio thread stays lock-free.
- RPC `audio.masterWindow` (`daw-rpc/src/audio.rs`), added with `add_binary`: the interface reads it as bytes (`RpcClient.callBinary`, decoded in `api/wire.ts`), AI clients as JSON. The mixer role reads it (`mixer/master.ts: MasterActions.masterWindow`, `useMixer().master`), mock `mock/handlers/audio.ts` + `mock/liveSound.ts` (synthesized "music" at the mock transport position, encoded by `mock/wire.ts`).
- UI: the component polls while it is shown (`core/poller.ts: createWatchedPoll`, with no `watch`); the spectrum and oscilloscope math is the pure `mixer/visualizer.ts`.
