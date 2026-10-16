# `daw-synth`

The built-in synth (`InstrumentFactory`; see "Built-in synth").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md).

## Built-in synth

The instrument of kind `synth` (`daw_contracts::SYNTH_KIND`), the starter beat's bass: a polyphonic subtractive synth. `src/engine.rs` is the sound (pure math with tests), `src/lib.rs` the `Instrument` glue (parameters in real units, smoothing so knob moves and automation do not click, notes and controllers); tests in `src/tests.rs`.

* **Envelope graph.** The parameter names `attack`/`hold`/`decay`/`sustain`/`release` are the convention `ui/tracks/EnvelopeGraph.tsx` (`domain/envelope.ts`) draws for any instrument; a new instrument gets the graph by naming its envelope the same way.
* **Picture.** For kind `synth` the instrument window also shows `ui/tracks/SynthGraph.tsx`: one cycle of the waveform and the filter response; `domain/synth.ts` mirrors the engine's math, so a change to the sound in `engine.rs` needs the same change there.
* **A similar instrument:** the [add-instrument](../../.claude/skills/add-instrument/SKILL.md) recipe.
