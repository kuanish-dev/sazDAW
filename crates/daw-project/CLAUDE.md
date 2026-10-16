# `daw-project`

**Core** (`ProjectService`; `HistoryControl`, `ProjectView`): the sole owner of the model and the edit log — the edit pipeline, undo/redo and edit groups, reconciling DSP objects, publishing snapshots, reading the whole project (see "Project core" below). Each job on the project is its own entity on top of the core (one file each, contracts in `daw-contracts`):

- **`ProjectTracks`** — tracks and their instruments (track order is display only; everything is tied to ids).
- **`ProjectTakes`** (`TakeControl`) — takes of an audio clip; rules in `take_rules.rs`.
- **`ProjectTempo`** (`TempoControl`) — the tempo map, see [tempo-map](../../docs/features/tempo-map.md).
- **`ProjectClips`** — clips on the timeline and the loop; batch edits are one undo step checked against the resulting state. Shared rules in `clip_rules.rs`, tempo fitting and pitch in `warp_rules.rs`.
- **`ProjectNotes`** — a clip's notes; content rules in `note_rules.rs` (shared with `ProjectClips`); musical edits in `note_transforms.rs`, `groove.rs`, `variation.rs`, see [note-edits](../../docs/features/note-edits.md).
- **`ProjectNoteEffects`** — a track's MIDI effects before the instrument (`note_effects.rs`).
- **`ProjectMarkers`** (`MarkerControl`) — section markers. **`ProjectFolders`** (`FolderControl`) — track folders; rules in `folder_rules.rs`.
- **`ProjectGraph`** — the processing graph: effects, buses, connections. **`ProjectStage`** (`StageControl`) — `stage.rs`, see [virtual-stage](../../docs/features/virtual-stage.md).
- **`ProjectAutomation`** — automation lanes. **`ProjectPluginStates`** (`PluginStates`) — saved plugin states, see [daw-vst3](../daw-vst3/CLAUDE.md).
- **`ProjectFiles`** (`ProjectIo`) — create/open/save/restore on top of the service, the storage and the recent list.
- **`ProjectSampleCatalog`** (`SampleCatalog`) — which sounds the project has; removal keeps the data in memory for undo.
- **`ProjectSampleEditor`** (`SampleEditor`) — everything we do with one sound.
- **`ProjectClipProcessing`** (`ClipProcessing`) — normalize and reverse, see [audio-clip-editing](../../docs/features/audio-clip-editing.md).
- **`ProjectMidiMappings`** (`MidiMappingControl`) — `midi_mappings.rs`, see [midi-controllers](../../docs/features/midi-controllers.md).
- **`ProjectCleaner`** (`ProjectClear`) — remove all content, keeping the project; one undo step (`clear.rs`).
- **`ProjectBounce`** (`TrackBounce`) — bounce and freeze, see [bounce](../../docs/features/bounce.md).

Shared lookups and small model edits are pure functions in `ops.rs`.

Depends on: contracts.

Map of the whole code: [ARCHITECTURE.md](../../ARCHITECTURE.md). Cross-layer features it takes part in: [audio-clip-editing](../../docs/features/audio-clip-editing.md), [automation](../../docs/features/automation.md), [bounce](../../docs/features/bounce.md), [channel-utility](../../docs/features/channel-utility.md), [cue](../../docs/features/cue.md), [markers-colors-folders](../../docs/features/markers-colors-folders.md), [midi-controllers](../../docs/features/midi-controllers.md), [mix-check](../../docs/features/mix-check.md), [note-edits](../../docs/features/note-edits.md), [recording](../../docs/features/recording.md), [sends](../../docs/features/sends.md), [sidechain](../../docs/features/sidechain.md), [song-loop](../../docs/features/song-loop.md), [tempo-map](../../docs/features/tempo-map.md), [virtual-stage](../../docs/features/virtual-stage.md), [warping](../../docs/features/warping.md).

## Project core (`src/service/`)

* **Whole-project reads — `ProjectView`** (`controls.view`), implemented by the core itself; used by `project.get`, export, autosave and telemetry. `ProjectIo` is only the file lifecycle.
* **Tempo, time signature and the loop are project data**: undoable, they travel to the engine in the snapshot; the transport does not store tempo (the loop is only passed to it).
* **Edit groups and checkpoints** (`HistoryControl`, `service/history.rs`): the semantics are documented on the trait in `daw-contracts/src/history.rs`. A group is shared by the whole project and blocks undo while open; `cancel_group` is how a part build, cutting sample regions and the starter beat roll back. Checkpoints (`checkpoint`/`squash`) make many separate calls (an AI request over MCP) one undo step after the fact without holding anything open; RPC `history.checkpoint`, `history.squash`, and the MCP instructions ask the AI to wrap each request in them.
* **DSP reconciliation** (`service/sync.rs`): create missing objects → publish the snapshot → remove stale ones, so the engine never sees a node without its object. Failures go to the log and the project warnings and are not retried until the kind/parameters change or a project opens.
* **Limits** are contract constants next to the traits (`MAX_TRACKS`, `MAX_CLIPS`, `MAX_NOTES_PER_CLIP`, …); implementations take them from there.
* Removing a sample resets instrument parameters of kind `ParamKind::Sample` found in the catalog, not by name.
* **The history as a list** (`HistoryControl::steps`, only reads): the labels of the done steps (oldest first) and of the undone ones (the next redo first); a squashed AI request is one step under its label. RPC `history.list`. The interface shows it in the History panel next to undo/redo (`app/src/ui/transport/HistoryPanel.tsx`): `actions.history.steps` is re-read with the project status, and a click walks to a step with `actions.history.travel`, which repeats the ordinary `history.undo`/`history.redo`; the browser mock lists its own stacks (`app/src/mock/handlers/project.ts`).
