# `daw-engine`

Real-time audio engine: playback of tracks' note and audio clips, graph processing in topological order from the snapshot (sources → effects and buses in any order → master), sidechain, automation, meters, the master scope for the visualizer, the node tap for the effect visuals and the meter tap for the live loudness meter (`scope.rs`), the mono check on the output (see "Live master meter" ([docs/features/master-meter.md](../../docs/features/master-meter.md))), the channel utility (`mixer/utility.rs`: polarity, width, mono, fine time alignment, see "Channel utility" ([docs/features/channel-utility.md](../../docs/features/channel-utility.md))), the virtual stage (`stage/`: a channel's place in the shared room, its direction cue — a parametric HRTF on headphones — and the room's tail, see "Virtual stage" ([docs/features/virtual-stage.md](../../docs/features/virtual-stage.md))).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [audio-clip-editing](../../docs/features/audio-clip-editing.md), [automation](../../docs/features/automation.md), [channel-utility](../../docs/features/channel-utility.md), [master-meter](../../docs/features/master-meter.md), [master-visualizer](../../docs/features/master-visualizer.md), [midi-controllers](../../docs/features/midi-controllers.md), [pdc](../../docs/features/pdc.md), [processing-graph](../../docs/features/processing-graph.md), [recording](../../docs/features/recording.md), [sends](../../docs/features/sends.md), [sidechain](../../docs/features/sidechain.md), [virtual-stage](../../docs/features/virtual-stage.md), [warping](../../docs/features/warping.md).

## Code layout (`src/`; modules by role are listed in `lib.rs`)

- `engine.rs` — the audio thread, the segment conductor.
- `link/` — the control side's view of the engine: `EngineLink` (snapshots and the "graveyard", DSP commands, the `Garbage` release queue, live notes and recording, `MonoCheck`), `telemetry.rs` (`EngineStats`, `NodeScope`, `MasterTap`, `Loss` counters), `monitor.rs` (the input table), `held.rs` (lanes held by live automation writing).
- `meters.rs`, `scope.rs` (`MasterScope`, `NodeTap`, `MeterTap`), `instruments.rs` (`InstrumentRack`), `note_effects.rs` (track MIDI effects), `playback.rs` (`notes.rs`, `controllers.rs`, `audio_voices.rs`), `automation.rs`, `params.rs`, `monitor.rs` (input monitoring), `preview.rs`.
- `mixer/` — `mod.rs` (node tables, `process`), `effects.rs` (`EffectPool`), `compile.rs` (graph snapshot → tables; also where automation overrides volume and pan for the segment), `utility.rs`, `latency.rs` + `delay.rs` (see [pdc](../../docs/features/pdc.md)).

**Rules across modules:**
- DSP objects arrive already prepared: `EngineLink::send` calls `prepare` on the control side; the mixer never prepares a received object on the audio thread.
- On a new snapshot, objects receive only the parameters that changed (`params.rs`); preview commands do not touch parameters.
- The frame of an event within a segment is one formula, `BlockTiming::frame_at`, for notes, audio clips and the metronome.

## Releasing memory on the audio thread

Everything the audio thread might drop as the last owner (an instrument, an effect, a preview sound, an audio clip voice's sound) goes into the `Garbage` queue (`GarbageBin::throw`); the control side releases it with `DspSink::collect_garbage` — when sending a command, in the `daw-engine-watch` thread (`daw-bootstrap/src/engine_watch.rs`, so the queue does not overflow during long playback without edits) and in offline rendering on every chunk. The same watch thread logs (`source: "engine"`) every increase of `EngineStats::losses`. A queue overflow is a leaked reference (`EngineLosses::leaked_garbage`), never a release on the audio thread.
