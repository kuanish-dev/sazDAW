# `daw-strings`

Built-in plucked strings (`InstrumentFactory`): acoustic and electric guitar, ukulele, bass guitar (see "Built-in strings").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Built-in strings

Kinds `guitar`, `electric_guitar`, `ukulele`, `bass_guitar` — constants `GUITAR_KIND`, `ELECTRIC_GUITAR_KIND`, `UKULELE_KIND`, `BASS_GUITAR_KIND` in `daw_contracts`, because the chord part builder (`daw-patterns/src/voicing.rs`) voices chords by them. One physical model, no sound files: extended Karplus–Strong in `src/engine.rs` (pure math, its steps described at the top of the file). The kinds differ only by body/pickup resonances, pick position, default parameters and whether they have an amp's `drive` — the `KINDS` table in `src/lib.rs`, which is also the `Instrument` glue (parameters, smoothing, notes).

* **Limitation:** a string's period is set when it is plucked, so `ring` and `tone` apply from the next pluck and controllers (pitch bend included) are ignored.
* **Realtime:** every voice's delay line is allocated when the instrument is created; notes and rendering never allocate.
* **Texts for the AI** (see "Sound texts for the AI" in [daw-contracts](../daw-contracts/CLAUDE.md)): each kind's `about` in `KINDS` (its sound, then the shared `plays_notes!` text on velocity and note length) and each parameter's `about` in `params()`. They describe `engine.rs`: a change to the sound there needs the same change in the text.
* **Frontend:** the kinds and defaults are copied into `app/src/mock/catalog.ts` (`plucked`); a new track's step-input pitch is in `instruments/strings.ts`.
* **A new string kind:** a row in `KINDS` (`drive: Some(…)` if it has an amp, its `about`), its constant in `daw_contracts::instrument` if the chord part should know it (and a row in `voicing.rs: style`), the mock catalog row, the labels in `i18n` (`catalog.kinds`). General steps: the [add-instrument](../../.claude/skills/add-instrument/SKILL.md) recipe.
