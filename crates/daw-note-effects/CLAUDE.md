# `daw-note-effects`

**MIDI effects** (`NoteEffect`): arpeggiator (`arpeggiator.rs`) and chord (`chord.rs`), the `NoteEffects` kind catalog (`NoteEffectCatalog`; see "Track MIDI effects").

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md).

## Track MIDI effects: arpeggiator and chord

A chain of MIDI effects sits on an instrument track **before the instrument**: clip notes and live notes (keyboard, `live.*`) pass through it in order, and the instrument receives the output of the last effect.

* **Contract** — `daw-contracts/src/note_effect.rs`: `NoteEffect` (the realtime side; its doc comment holds the output rules every kind follows), the catalog `NoteEffectCatalog`, chain editing `NoteEffectControl` (one undo step per call, a moving knob merges). Model `Track::note_effects: Vec<NoteEffectSpec>` (JSON `noteEffects`, omitted when empty; ids from the project's shared counter).
* **Implementations** (this crate) — `arp` and `chord`, params in each module's `PARAMS`. The arpeggiator's steps lie on the transport grid, so they stay in time, in export too; while stopped it runs a free clock started by the first key. The chord remembers what each key played and counts shared pitches, so a release does not depend on later param changes.
* **Engine** — `daw-engine/src/note_effects.rs` (`NoteEffectRack`) runs a segment's notes through the chains (bypassed effects skipped); `engine.rs: play_events` hands the result to the instruments by frame, a controller on the same frame first. No stuck notes: on a chain change (`DspCommand::SetNoteEffects`) or a bypass change the rack releases what the chain pressed, resets the effects and feeds the held keys again; stop and seek reset everything and `all_notes_off` silences the instrument.
* **Core** — `daw-project`: `ProjectNoteEffects` (`note_effects.rs`); `service/sync.rs: sync_note_effects` rebuilds a chain only when its ids or kinds change. Params and bypass travel with the snapshot, and the engine applies only the changed ones (`daw-engine/src/params.rs: changed`), as for instruments. An effect that fails to create is left out of the chain (`notes::NOTE_EFFECT_NOT_CREATED`, a `dsp` log entry and a warning); the rest work.
* **Export and bounce** — the offline engine (`daw-render`, `load_engine`) builds the same chains: export sounds like playback (`daw-bootstrap/tests/all/render_note_effects.rs`).
* **RPC** — `noteEffects.add {track, kind, params?}` → id, `noteEffects.remove/move/setParam/setBypass {track, effect, …}` (`daw-rpc/src/note_effects.rs`); `catalog.get` → `noteEffects`. **Interface** — the "MIDI effects" section of the instrument window (`ui/tracks/NoteEffectsSection.tsx`), the chain badge in the track header (`domain/noteEffects.ts: chainBadge`); actions `actions/noteEffects.ts`, mock `mock/handlers/noteEffects.ts` (model only, no sound).
* **Limits.** What is recorded is what was played (the chain input), not its output; MIDI export is also the clip notes. MIDI effect parameters have no automation or MIDI learn (there is no `ParamTarget` for them). The arpeggiator has no latch. At most `MAX_NOTE_EFFECTS` (8) per track. Stop and seek cut off the arpeggio even from held live keys (as with the instrument's sound): the key has to be pressed again. Changing the chain's composition resets its state (the arpeggio pattern starts over).
* **How to add a similar one** — a new MIDI effect kind: a module in `daw-note-effects` that implements `NoteEffect` and follows its output contract (by frame, a release before a press on the same frame, every press of its own released by itself), a row in `KINDS` (`lib.rs`), a copy of the kind in `app/src/mock/catalog.ts`, the badge in `domain/noteEffects.ts`.
