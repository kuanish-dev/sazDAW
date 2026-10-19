# Melodic step lines and the song key

A melody written the way a step sequencer holds it — an acid riff, a bass line, an arpeggio — instead of a list of notes with ticks; and the song's key, in which such a line can count scale degrees. Made for the AI (MCP): before this, a 303 line had to be turned into notes by a script of its own. Drums have the same kind of notation already (`patterns`: `X... x...`).

**The notation** (rules at the top of `daw-contracts/src/step_line.rs`): one token per step, separated by spaces, `|` marks a bar and is ignored. A step is a note name (`F2`, `Eb3`, `c#4`; C4 = 60) or a scale degree of the key (`1` the tonic in the octave `octave`, `8` the tonic an octave up, `+`/`-` after it an octave up/down), then `!` for an accent and `~` for a slide; `-` or `.` is a rest, `_` holds the note before it. Example, an F minor riff on sixteenths:

```
F2 F3! - F2 Eb3~ F3 - C3 | F2 _ Ab2! - C3~ Eb3 F2 -
```

- **Lengths.** A note sounds `gate` (0.5 by default) of its last step. `~` makes it sound for all its steps and into the next note: into the same pitch it is a tie (one note), into another pitch the two overlap by at most `SLIDE_OVERLAP_TICKS` (a 64th, never more than half of the next note's gate), which is what makes a mono synth with glide slide (`daw-synth`: a note-on while a note is held is a legato slide). Before a rest or at the end of the line a slid note just fills its steps — the line loops, and a note cannot run past the content.
- **Velocities** are the drum grid's: `VELOCITY_NORMAL`, and `VELOCITY_STRONG` for `!` (`daw-contracts/src/patterns.rs`).
- **Contract.** `ClipControl::add_step_clip(NewStepClip)` (`daw-contracts/src/clips.rs`); the parsing is the pure `NewStepClip::notes(key)` in `step_line.rs`, with its tests. The note letters are shared with chord symbols (`chord_part.rs: letter_pitch_class`). Errors are codes `PT-104`…`PT-106`, `PT-276`, `PT-277`, `PT-371` (`codes/patterns.rs`) and `notes::PITCH_OUT_OF_RANGE`; lengths are checked by the clip like any note clip (a line that is not a whole number of sixteenths is `notes::LOOP_LENGTH_OUT_OF_RANGE`).
- **Implementation** (`daw-project/src/clips.rs: add_step_clip`): one core edit reads the key, turns the line into notes and places the clip with `place_note_clip`, the same path as `add_note_clip` — one undo step, `loop_ticks` = the line's length, the clip one pass unless `lengthTicks` asks for repeats. Tests: `daw-project/src/tests/song_key.rs`.

**The song key** (`Project::key: Option<ScaleKey>`, the root 0…11 and a `Scale`; none in a new project). Its job is `KeyControl` (`daw-contracts/src/song_key.rs`), implemented by `ProjectKey` (`daw-project/src/song_key.rs`): project data like the tempo map — saved, undoable, sent to the window by the project feed (`ProjectChanges::key`, mirrored in `app/src/core/projectChanges.ts` and `api/types/model.ts`). It changes no sound. `project.clear` keeps it (like the name); templates keep it (like the tempo). The project summary (`project.summary`) shows it.

- **RPC** (`daw-rpc/src/key.rs`, `clips.rs`): `key.get`, `key.set { root, scale = minor }`, `key.clear`, `clips.addSteps`; the MCP instructions point the AI at `clips_addSteps` and `key_set`. Wire tests: `daw-rpc/tests/all/steps.rs`.
- **Fit to the key.** `notes.fitToScale` already moves notes to a scale (`NoteTransform::FitToScale`, see [note-edits](note-edits.md)); it takes the root and scale as params, so the AI passes the key it set.
- **Frontend.** Only the data: the interface's project copy has `key`, the browser mock's projects carry `key: null`. There is no key picker or display yet, and `clips.addSteps` / `key.*` have no `api.ts` wrappers or mock handlers — like the other AI-only methods (`clips.addNoteMany`, `analysis.*`), they come when an interface uses them.

**A new mark** (for example a per-step gate): a branch in `read_step` and `Line::close` in `step_line.rs` with a test, a line in the `steps` field doc and the `clips.addSteps` description, and the `BAD_STEP` template (with its Russian text in `app/src/i18n/errors/ru.ts`).
