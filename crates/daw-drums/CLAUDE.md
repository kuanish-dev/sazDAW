# `daw-drums`

Built-in drums (`InstrumentFactory`); 11 drums: kick, snare, hat, clap, openhat, tom_hi/mid/lo, crash, ride, rim.

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Built-in drums

Each drum is one synthesized voice in `src/voice.rs` (tones, noise and exponential envelopes; the note's pitch and length are ignored, a new note restarts the hit). `src/lib.rs` holds the parameter table `PARAMS` (`gain`, `tune`, `decay`, the same for every kind) and the catalog `DRUMS`, one `DrumFactory` per kind.

* **Texts for the AI** (see "Sound texts for the AI" in [daw-contracts](../daw-contracts/CLAUDE.md)): each drum's `about` in `DRUMS` says what it sounds like and ends with the shared `plays_notes!` text (velocity, note pitch and length); each parameter's `about` in `PARAMS`. They describe the numbers in `voice.rs`: a change to a drum's sound needs the same change in its text.
* **A new drum:** a `DrumKind` variant, its sound and length in `voice.rs`, a `DrumFactory` row in `DRUMS` with its `about`; then the mock catalog row and labels (the [add-instrument](../../.claude/skills/add-instrument/SKILL.md) recipe, steps 3–5).
