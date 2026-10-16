# Master visualizer

**Master visualizer** (spectrum and oscilloscope on the master strip, `ui/mixer/MasterVisualizer.tsx`).

- Contract: `EngineStats::watch_master(on)` and `master_window() → MasterWindow` (`daw-contracts/src/engine.rs`, `MASTER_WINDOW_FRAMES`).
- Engine: `daw-engine/src/scope.rs: MasterScope` in `Telemetry` — a preallocated ring of atomics that the audio thread fills in `process_segment` only while watched, after the preview voice and before the ±1 clamp and the metronome (the picture shows the music, not the clicks); the control side copies it out, so the audio thread stays lock-free.
- RPC `audio.watchMaster` / `audio.masterWindow` (`daw-rpc/src/audio.rs`), actions `audio.watchMaster` / `audio.masterWindow`, mock `mock/liveSound.ts` (synthesized "music" at the mock transport position).
- UI: the component turns capture on while it is shown and polls it (`state/poller.ts: createWatchedPoll`); the spectrum and oscilloscope math is the pure `domain/visualizer.ts`.
- Limitation: there is one switch for the app; a second visualizer would need a watcher count in `MasterScope`.
