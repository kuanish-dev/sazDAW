# `daw-strings`

Built-in plucked strings (`InstrumentFactory`): acoustic and electric guitar, ukulele, bass guitar (see "Built-in strings").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Built-in strings (`daw-strings`)

Kinds `guitar` (acoustic guitar), `electric_guitar`, `ukulele` and `bass_guitar` (`GUITAR_KIND`, `ELECTRIC_GUITAR_KIND`, `UKULELE_KIND`, `BASS_GUITAR_KIND` in `daw_contracts`: the chord part builder voices chords by them). One physical model, no sound files: **extended Karplus–Strong** (`src/engine.rs`, pure math with tests). A string is a delay line one period long; a pluck fills it with a noise burst — darkened by a one-pole lowpass (brighter with `tone` and a harder pluck), without the harmonics that have a node at the pick position, zero-mean; every trip around the loop passes a two-tap lowpass (high harmonics die first), a first-order allpass (the fractional part of the period, so every note is in tune) and a loss factor (`ring`: seconds until −60 dB for a string near G3, shorter for higher strings). Releasing a note damps the string in 0.12 s like a finger or a palm (a short note is a muted "chuck"). The same note re-plucks its own string; 12 voices, otherwise the quietest is stolen. The voices are summed and a **body** adds two resonances (bandpasses; on the electric kinds a magnetic pickup's — the electric guitar's bright peak near 3 kHz): what makes the kinds differ, together with the pick position and the default parameters (`KINDS` in `src/lib.rs`). The kinds played through an amp (the electric guitar and the bass) have an **overdrive** after it: a `tanh` soft clipper whose input gain grows with `drive` (up to ×21), divided by that gain: quiet playing keeps its level and loud playing is squashed, so the loudness stays near the clean sound's (a little lower at full drive); `drive` 0 skips it.

* **Parameters:** `ring` (s), `tone` (brightness, %), `body` (%), `gain` (volume, %), and on the kinds with an amp `drive` (%, 0 by default: the electric guitar starts clean). `body`, `drive` and `gain` glide over ~20 ms; `ring` and `tone` apply from the next pluck (a string's loop is set when it is plucked). Controllers, pitch bend included, are ignored for the same reason.
* **Realtime:** every voice's 8192-sample delay line (the longest period: 23 Hz even at 192 kHz) is allocated when the instrument is created; `note_on` (filling one period) and `render` never allocate.
* **Frontend:** the kinds and defaults are copied into `app/src/mock/catalog.ts` (`plucked`); a new track's step-input pitch — `domain/strings.ts` (E3 for both guitars, C4, A1).
* **A new string kind:** a row in `KINDS` (body modes, pick position, defaults, `drive: Some(…)` if it has an amp), its constant in `daw_contracts::instrument` if the chord part should know it, the mock catalog row, the labels in `i18n` (`catalog.kinds`).
