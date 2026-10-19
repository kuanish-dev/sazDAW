# `src/core/` — what every role shares

The core is to the roles what the project core is to the backend entities: each role holds it and asks it (`Core` in `core.ts`, made by `createCore`).

- **The backend:** `api` (typed wrappers over `RpcClient`), system dialogs, the window shell.
- **The project copy** (`projectSync.ts`, `ProjectCopy`): the project as the interface has it, its status and the kinds catalog. On the backend's change event it takes in only what changed since its revision (`project.changes`, taken in by `projectChanges.ts`); the first read and a re-read after a refused call take the whole project. `patchProject` shows a value at once (a slider) before the backend replies; only roles call it, never a component.
- **Calls** (`calls.ts`): `run`, `attempt`, `exec` — a failure is a toast (`notifications.ts: showFailure`) and a whole re-read; `withBusy` shows the loading indicator.
- **Playback** (`playback.ts`): the `tick` stream and the positions for drawing.
- **Polling** (`poller.ts`): one request at a time; `createWatchedPoll` for a backend capture that is open only while it is polled.

The UI never gets the core, only what it may read of it (`CoreReads`). The core imports no role: roles ask the core, not the other way round (checked by the linter).

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md). Cross-layer features it takes part in: [errors](../../../docs/features/errors.md), [song-loop](../../../docs/features/song-loop.md).

**Playback position for drawing** (`core/playback.ts`). Canvases (timeline, piano roll, steps, automation, sample editor) read `session.playhead()` / `previewPlayhead()`, not `tick()`: the tick arrives ~30 times per second even when stopped (meters are live), the derived positions fire only on a change. Light per-frame elements — the timeline cursor (a separate element moved with `translateX`, so the canvas is not redrawn during playback), the glow layer, the beat lights — read `session.smoothPlayhead()`, extrapolated between ticks by the pure `core/playhead.ts`; it waits at the end of `tick.wrap` (the loop region or the song, see [song-loop](../../../docs/features/song-loop.md)) until the jump arrives. A new canvas that needs the position takes it from here.
