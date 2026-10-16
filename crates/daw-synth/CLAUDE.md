# `daw-synth`

The built-in synth (`InstrumentFactory`; see "Built-in synth").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md).

## Built-in synth (`daw-synth`)

The instrument of kind `synth` (`daw_contracts::SYNTH_KIND`): new tracks' default synth and the starter beat's bass. 8 voices (a free voice, otherwise the oldest), waveform (`ParamKind::Choice`: Sine/Triangle/Saw/Square), AHDSR envelope, a one-pole highpass `low_cut` (20…2,000 Hz; 20 Hz leaves the sound untouched) and a one-pole lowpass `high_cut` (40…18,000 Hz; the mod wheel opens it up to 4 octaves), `gain` in %. `src/engine.rs` is the sound (pure math with tests), `src/lib.rs` the `Instrument` glue: parameters in real units, both cuts (in octaves) and volume glide over 20 ms so knob moves and automation do not click, no allocations or locks in `render`.

* **Envelope graph.** The parameter names `attack`/`hold`/`decay`/`sustain`/`release` (seconds; `sustain` a level 0…1) are the convention `ui/tracks/EnvelopeGraph.tsx` (`domain/envelope.ts`) draws for any instrument; a new instrument gets the graph by naming its envelope the same way.
* **Picture.** For kind `synth` the instrument window also shows `ui/tracks/SynthGraph.tsx`: one cycle of the waveform and the filter response with both cut markers (`domain/synth.ts` mirrors the engine's math).
* **How to add a similar instrument:** a crate depending only on `daw-contracts` with an `InstrumentFactory` (see `daw-sampler`, `daw-drums`), a `register` line in `daw-bootstrap/src/wiring/catalogs.rs`, the same kind and parameters in `app/src/mock/catalog.ts`.
