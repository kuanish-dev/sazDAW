# `src/ui/effects/` — effect editors and their live graphs

Map of the whole code: [ARCHITECTURE.md](../../../../ARCHITECTURE.md). Cross-layer features it takes part in: [sidechain](../../../../docs/features/sidechain.md).

## Live effect visuals

Every built-in effect's editor shows a graph of what it does, drawn from its params (so it follows the knobs at once, even while stopped), with the node's live sound over it that fades out when playback stops; the Graph page's signal path view shows every effect along a path at once. `EffectGraph.tsx` picks the graph by kind; `EffectPanel` and the signal path view use it.

**The live sound of a node.** Contract `NodeScope` (`daw-contracts/src/engine.rs`, `Controls.node_scope`): `watch_nodes(nodes)` and `node_windows(seen: [NodeCursor]) → [NodeWindow]`. Its doc comments say why the window carries the node's input as well as its output and why a watched set with cursors. Decisions beyond them:
* `NODE_WINDOW_FRAMES` is 4096, not 2048: 2048 would leave the 20–150 Hz end of the EQ graph a handful of bins.
* The watched set belongs to the whole app and is replaced as a whole: the interface sends the union of every picture it shows, so two pictures of one node cannot stop each other.
* Cursors live with the reader, so a read changes nothing in the engine and several readers (the interface, an AI client over MCP) do not disturb each other.
* Its own contract, not more `EngineStats`: watching nodes is a job of its own, and the planned `EngineLink` split gives telemetry its own traits anyway.
* Limitation: the window is mono (the middle of the channels), so a hard-panned sound reads up to 6 dB below what an effect's detector sees, and a sidechain key is not captured (a keyed compressor or gate shows its own input; the gain reduction is still the engine's).

Engine: `daw-engine/src/scope.rs: NodeTap` (in `Telemetry`), `MAX_WATCHED_NODES` preallocated slots of atomics; `GraphMixer::process` offers each node's input before its effect or fader (`tap.input`) and its output after (`tap.output`). The audio thread costs one atomic load while nothing is watched and never allocates; the control side's mutex is never taken by it. RPC `audio.watchNodes { nodes }` / `audio.nodeWindows { seen }` (`daw-rpc/src/audio.rs`), actions `audio.watchNodes` / `audio.nodeWindows`, mock `mock/handlers/audio.ts` + `mock/liveSound.ts` (the mock runs no effects: input and output are the same music, so an EQ's spectrum ignores its bands and a distortion's trace lies on the 1:1 line).

**Frontend.** One registry for the app, `state/nodeSounds.ts` (`session.nodeSounds`): pictures `listen(node, hear)`; while anything listens it polls (`createPoller`), tells the engine the set when it changed (and again when a reply lacks a node: another client replaced the set) and reads every window in one call with its cursors. The last listener leaving stops polling and empties the set; a failed read (the action shows a toast) stops it until the next picture opens. Nodes beyond the first `MAX_WATCHED_NODES` sound silent. A picture listens through `ui/effects/nodeSound.ts: listenToNode(node, hear, onStop)`. Shared pure math in `domain/`: `liveLevels.ts` (newest peak/RMS, scrolling history, the follower that fades a picture out), `plot.ts`, `frequencyAxis.ts` (the log axis of the EQ-like graphs), `biquad.ts` (the mirror of `biquad.rs` for every biquad graph), `spectrum.ts` (`LiveSpectrum`, on the visualizer's FFT `visualizer.ts: magnitudes`), `bandGains.ts` (band gains held by `ui/effects/bandGains.ts: createBandGains`).

**Each effect's shape is a pure `domain/` module that mirrors its Rust DSP** (the Rust module names its mirror; change both sides together). Each component's header describes its picture:
* `eq` → `EqGraph.tsx` + `LiveSpectrum.tsx` (`eqCurve.ts` ↔ `eq.rs`): the spectrum is of the output, like Pro-Q by default — what you hear, so a boost or cut reshapes the spectrum under the curve.
* `filter` → `FilterGraph.tsx` (`filterCurve.ts` ↔ `filter.rs`) over the same `LiveSpectrum`.
* `compressor`, `limiter`, `gate` → `DynamicsGraph.tsx` (table `DYNAMICS`; geometry `dynamicsGraph.ts`; `compressorCurve.ts` ↔ `dynamics.rs: GainComputer::reduction`, `limiterCurve.ts`, `gateCurve.ts`): the gain reduction is the engine's (`tick.reductions`), with the held `ReductionMeter`.
* `distortion`, `saturator`, `tape` → `ShaperGraph.tsx` (table `SHAPERS`; `shaperCurve.ts`): the curve is the static shaper only — the tone filter, oversampling, DC blockers, the tape's head bump and roll-off and the console's crosstalk act over time and show only in the live X-Y trace.
* `delay` → `DelayGraph.tsx` (`delayTaps.ts` ↔ `delay.rs`): the time from `tick.tempo`, like the engine's `BlockTiming`.
* `chorus` → `ChorusGraph.tsx` (`chorusSweep.ts` ↔ `chorus.rs`): the engine does not report its LFO phase, so the picture turns its own at the same rate — right speed and range, not the engine's exact moment.
* `reverb` → `ReverbGraph.tsx` (`reverbTail.ts` ↔ `reverb.rs`): the reverb's own level is `output − input × (1 − mix)`, exact because that is the dry part of `Reverb::process`.
* `multiband` → `MultibandGraph.tsx` (`multibandCurve.ts` ↔ `multiband.rs`, `crossover.rs`) and `dynamic_eq` → `DynamicEqGraph.tsx` (`dynamicEqCurve.ts` ↔ `dynamic_eq.rs`): the live gains are the engine's band gains (`tick.bands`, held by `createBandGains`), not measured from the window.
* `de_esser` → `DeEsserGraph.tsx` (`deEsserCurve.ts` ↔ `de_esser.rs`): the response now comes from the engine's gain reduction (`session.reductions`).
* `transient` → `TransientGraph.tsx` (`transientShape.ts` ↔ `transient.rs`: the same followers run on a model hit, recomputed on every knob move).
* `linear_eq` → `LinearEqGraph.tsx`: `EqGraph` (same params, so the same curve) with the latency at the stream's rate (`linearPhase.ts` ↔ `kernel_len`, `Convolver::latency`; the rate from `ui/common/streamRate.ts`).

**A new effect's picture:** step 5 of the recipe [add-effect](../../../../.claude/skills/add-effect/SKILL.md) — a `Match` and the kind in `OWN_GRAPHS` in `EffectGraph.tsx`, or an entry in `DYNAMICS`/`SHAPERS`. Settings come by param name through `catalog.ts: namedParams`, so a changed catalog shows no graph rather than a wrong one. Tests: `daw-engine/src/scope.rs` (lanes and slots), `daw-engine/src/tests/scope.rs` (taps in a running graph), `daw-rpc/tests/all/audio.rs`; the frontend has no tests yet.
