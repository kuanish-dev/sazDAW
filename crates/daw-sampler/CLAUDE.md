# `daw-sampler`

The built-in sampler (`InstrumentFactory`).

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Built-in sampler

Kind `sampler` (`daw_contracts::SAMPLER_KIND`): plays a sample from the project library (`sample` holds its id; changing it recreates the instrument, `rebuild_on`) on notes, 16 voices, the pitch following the note from `root`, over a region (`start`…`end`), forwards or backwards, once or looped. All of it is in `src/lib.rs`: the parameter table `PARAMS`, the voices and the playback.

* **Texts for the AI** (see "Sound texts for the AI" in [daw-contracts](../daw-contracts/CLAUDE.md)): `ABOUT` and each parameter's `about` in `PARAMS` describe the playback in `src/lib.rs` (velocity, `one_shot` and `loop` together); a change to the playback needs the same change in the text.
