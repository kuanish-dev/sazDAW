# `daw-patterns`

**Parts from patterns**: `PatternBank` (`PatternLibrary`: ready-made drum patterns — grooves, fills, breaks, intros, endings; the table is in `data.rs`), `DrumPartBuilder` (`DrumPartControl`: turns a sequence of patterns into drum tracks and clips with notes in a single undo step) and `DrumPartPreviewer`; `ChordPartBuilder` (`ChordPartControl`: a chord progression strummed on one track) and `ChordPartPreviewer` (see "Chord parts"); `render.rs` renders and loops both previews.

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [tempo-map](../../docs/features/tempo-map.md).

## Drum patterns

A pattern is a drawing on a 16th-note grid, one row per drum, 1 or 2 bars long; the grid syntax and the roles are described in `data.rs` and `daw_contracts::patterns`. **A new pattern** = a new entry in `PATTERNS`; the `every_builtin_pattern_is_valid` test (`library.rs`) checks grid lengths, drums and duplicates.

**Building a part** (`part.rs`, `plan.rs`; contract `DrumPartRequest`): patterns are placed one after another from `startBar`. Everything is validated up front (patterns, drums, sounds, occupied space), then the edits go as one undo group; a failure midway rolls the group back, so a refused request leaves no traces. Each pattern and drum gets one note clip whose content is a single pattern (`loop_ticks`), so repeats loop inside the clip and editing one drawing changes all repeats. Occupied space: `replace: false` is refused, `true` deletes the drum clips in that range.

**What plays each drum** (`lanes` → `LaneTarget`, resolved by `LaneKit` in `kit.rs`): a pattern only says "kick, snare, hi-hat…"; the request picks a built-in drum (`auto`: your topmost track of that kind, or a new one), silence, your track, or a project sample (a new sampler track). Several drums sent to one track merge into one clip, because two note clips on one track must not overlap. The dialog remembers the choices between builds (`session.drums.kit`); a choice that no longer exists counts as `auto`.

**Preview** (`DrumPartPreviewer`, `render.rs`): plays a part without changing the project. It uses the same `LaneKit` as the build, so what you hear is what will be built; the part is rendered offline into a temporary buffer (instruments created through `InstrumentCatalog`) and looped on the engine's existing preview voice (`DspCommand::PreviewPlay`), with cymbal tails folded onto the loop start. A new `play` replaces the previous one; length is capped by `MAX_PREVIEW_BARS`.

- **RPC:** `patterns.list`, `patterns.get`, `drums.build`, `drums.preview` (the same request as `build`), `drums.stopPreview` (`daw-rpc/src/patterns.rs`).
- **UI:** the "Drum part" button above the timeline, `drums/DrumPartDialog.tsx`; the role `session.drums.*` (`drums/`).
- **Mock** (`mock/handlers/drums.ts`): knows only 8 patterns and builds a part with separate edits rather than one undo step.

Example of a song form for AI: `drums.build { "slots": [{"pattern":"intro_crash_start"},{"pattern":"rock_basic","repeat":6},{"pattern":"fill_tom_cascade"},{"pattern":"rock_drive","repeat":7},{"pattern":"fill_snare_roll"},{"pattern":"ending_crash_stop"}] }` (a request with `lanes` is in the tests of `daw_contracts::patterns`).

## Chord parts (`chord_part.rs`, `strum.rs`, `voicing.rs`)

A chord progression strummed on one instrument track. **Contract** (`daw-contracts/src/chord_part.rs`): `ChordPartRequest` (target track or a new one of a given kind, one or two chord symbols per bar read by `Chord::parse`, a strum, repeats, replace) and the strum table `STRUMS` (one 4/4 bar of eighth notes, named after the "бой четвёрка / шестёрка / восьмёрка" of the Russian guitar tradition). Like drum patterns, strums are not reshaped for another meter.

- **How the instrument plays** (`voicing.rs: style`, by the target's instrument kind): the guitars and the ukulele voice the chord like a player, the bass plays roots on down strokes, any other instrument a block chord.
- **Strumming** (`strum.rs`, the stroke rules are at its top). The clip's content is one pass (`loop_ticks`), so repeats loop inside it.
- **Build** (`ChordPartBuilder`): `chord_part::plan` checks the whole request first and works out the notes; then one undo group (the new track, the removed clips, one note clip); a failure rolls the group back. **Preview** (`ChordPartPreviewer`): the same plan played by the target's instrument (a new kind with its defaults) and looped by `render.rs`, shared with the drum preview.
- **RPC:** `chords.strums`, `chords.build`, `chords.preview`, `chords.stopPreview` (`daw-rpc/src/chords.rs`); the MCP instructions mention the workflow.
- **UI:** "Chord part" above the timeline, `chords/ChordPartDialog.tsx` (`ProgressionBars`, `ChordPalette`, `StrumPicker`); "▶ Listen" replays the part after every edit. Pure logic — `chords/chordPart.ts`; the role — `session.chords.*` (`chords/`).
- **Mock** (`mock/handlers/chords.ts`): writes block chords from C3 on every stroke, with separate edits.
- **Tests:** symbols and strums — `daw-contracts/src/chord_part.rs`; voicings — `voicing.rs`; strokes — `strum.rs`; one undo step, real session, occupied space, refusals, preview — `daw-bootstrap/tests/all/chord_part.rs`; wire — `daw-rpc/tests/all/chords.rs`.
- **A new strum:** a row in `STRUMS` (and the mock's `STRUM_GRIDS`). **A new string instrument:** its kind in `daw-strings` and a row in `voicing.rs: style`.
