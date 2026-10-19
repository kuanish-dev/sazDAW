# `src/core/` — what every role shares

The core is to the roles what the project core is to the backend entities: each role holds it and asks the part it needs (`Core` in `core.ts`, made by `createCore`). Its parts are separate objects, each with one job:

- **The backend:** `core.api` (typed wrappers over `RpcClient`), `core.dialogs` (system dialogs), `core.appShell` (window zoom and the app version).
- **The project copy** (`core.copy`, `projectSync.ts`, `ProjectCopy`): the project as the interface has it, its status and the kinds catalog. The project and its status come by themselves from the backend's project feed (`RpcClient.onProject`): the whole project first, then after each edit only what changed (taken in by `projectChanges.ts`); the copy keeps no revision. `reload` asks the feed for the whole project again (after a refused call), and `whenHas` waits until the copy has what a call just made (`views.showCreated`). `patchProject` shows a value at once (a slider) before the backend replies; only roles call it, never a component.
- **Calls** (`core.calls`, `calls.ts`): `run`, `attempt`, `exec` — a failure is a notification (`showFailure`) and the whole project asked for again (`copy.reload`); `withBusy` shows the loading indicator (`busy`).
- **Notifications** (`core.notifications`, `notifications.ts`): `toast`, `showFailure` (an error also goes to the log) and the list the window shows.
- **Playback** (`core.playback`, `playback.ts`): the `tick` stream and the positions for drawing.
- **Polling** (`poller.ts`): one request at a time; `createWatchedPoll` for a backend capture that is open only while it is polled.
- **The roles' registry** (`roles.ts`): `defineRole` (in each role's `role.ts`), `createRoles` (once, by `start.ts`), `useRole` (behind each role's hook) and the hooks for what the UI reads of the core — `useProjectCopy`, `usePlayback`, `useToasts`, `useBusy`. A role is made on its first use, under the window's root, from the core and the roles it asks; a cycle between two roles stops with both names.

The UI never gets the core, only what it may read of its parts, through the hooks above (each returns one part under its narrow type: `useProjectCopy` the copy as `ProjectReads`). The core imports no role: roles ask the core, not the other way round (checked by the linter).

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md). Cross-layer features it takes part in: [errors](../../../docs/features/errors.md), [song-loop](../../../docs/features/song-loop.md).

**Playback position for drawing** (`core/playback.ts`). Canvases (timeline, piano roll, steps, automation, sample editor) read `usePlayback().playhead()` / `previewPlayhead()`, not `tick()`: the tick arrives ~30 times per second even when stopped (meters are live), the derived positions fire only on a change. Light per-frame elements — the timeline cursor (a separate element moved with `translateX`, so the canvas is not redrawn during playback), the glow layer, the beat lights — read `usePlayback().smoothPlayhead()`, extrapolated between ticks by the pure `core/playhead.ts`; it waits at the end of `tick.wrap` (the loop region or the song, see [song-loop](../../../docs/features/song-loop.md)) until the jump arrives. A new canvas that needs the position takes it from here.
