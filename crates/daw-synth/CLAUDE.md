# `daw-synth`

The built-in synth (`InstrumentFactory`; see "Built-in synth").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [midi-controllers](../../docs/features/midi-controllers.md).

## Built-in synth

The instrument of kind `synth` (`daw_contracts::SYNTH_KIND`), the starter beat's bass: an 8-voice stereo subtractive synth. Two oscillators (sine, triangle, saw, square, noise) with up to seven detuned unison copies spread across the stereo field, an AHDSR volume envelope, a resonant 12 dB/octave lowpass per voice with its own attack–decay envelope, a low cut on the mix, a pitch envelope, mono mode with glide, and one LFO for all voices (pitch, filter, volume; in hertz or synced to the song grid).

Files by role (`src/`):
* `lib.rs` — the `Instrument` glue: the parameter table `PARAMS` (grouped as the window shows them), real units → `engine::Settings`, smoothing of the cuts and volume (so knob moves and automation do not click), notes, controllers and the song tempo (`set_timing` → a synced LFO).
* `engine.rs` — the voices: allocation (free, else oldest), mono mode (one voice, a stack of held keys: a key pressed while another is held slides to it without restarting the envelopes, letting go slides back to the newest key still held), the unison table (`Unison`), the per-frame mix. Pure math without the contracts; tests in `src/engine/tests.rs`.
* `osc.rs` — waveforms and the white noise generator (xorshift, the same sequence after every reset, so renders repeat exactly).
* `envelope.rs` — the AHDSR volume envelope and the filter's attack–decay envelope.
* `filter.rs` — the voice lowpass (a topology-preserving state-variable filter: stable while the cutoff moves every frame). Near its top (18 kHz) it lets a sharp signal overshoot up to ×1.31; tests allow for that.
* `lfo.rs` — LFO shapes (sine, triangle, saw, square, random) and the sync note lengths in beats.

Decisions:
* **Unison** keeps one copy as the plain oscillator in the centre (so `unison = 1` is a mono sound with both channels equal); more copies are detuned symmetrically, spread by `unison_width` and scaled by `1/√n` so the loudness stays about the same.
* **Glide** works only in mono mode and only between held keys (legato): an 808 pattern of separate notes does not slide, overlapping notes do.
* **A synced LFO** is put back on the grid at every segment start while playing (`Engine::set_timing`), so it stays on the beat after a seek or a loop; stopped, it runs at the last tempo.

* **Envelope graph.** The parameter names `attack`/`hold`/`decay`/`sustain`/`release` are the convention `instruments/EnvelopeGraph.tsx` (`instruments/envelope.ts`) draws for any instrument; a new instrument gets the graph by naming its envelope the same way.
* **Picture.** For kind `synth` the instrument window also shows `instruments/SynthGraph.tsx`: one cycle of the waveform and the filter response with its resonance peak; `instruments/synth.ts` mirrors the math of `osc.rs` and `filter.rs`, so a change to the sound there needs the same change in that file.
* **Velocity** scales only the volume (linearly); it does not reach the filter, so there is no TB-303-style accent. `ABOUT` says so to the AI.
* **Texts for the AI** (see "Sound texts for the AI" in [daw-contracts](../daw-contracts/CLAUDE.md)): `ABOUT` for the synth and each parameter's `about` in `PARAMS` describe the sound in `engine.rs`, `filter.rs`, `envelope.rs` and `lfo.rs` (scales, what velocity changes, when glide slides); a change to the sound there needs the same change in the text.
* **Adding a parameter:** a line in `PARAMS` (with its `about`), an arm in `Synth::set_param` that writes `Settings`, the use in `engine.rs`; then the same line in the browser mock (`app/src/mock/catalog.ts`, label in `i18n` `catalog.params`) and the count in `daw-bootstrap/tests/all/plugins.rs`.
* **A similar instrument:** the [add-instrument](../../.claude/skills/add-instrument/SKILL.md) recipe.
