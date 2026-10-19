# Frontend (`app/`: Solid.js + TypeScript + Tailwind v4, Tauri 2 shell)

The interface talks to the backend through a single contract, `RpcClient` (`src/api/rpc.ts`): `call(method, params)`, `callBinary(method, params)` for the live sound windows (raw bytes, decoded in `src/api/wire.ts`; see "Bytes" in [daw-rpc](../crates/daw-rpc/CLAUDE.md)) plus two subscriptions —
`tick` (position, meters, load, ~30 times per second) and `project` (the project changed → take in what changed since the copy's revision, `project.changes`; see [src/core/](src/core/CLAUDE.md)).

Map of the whole code: [ARCHITECTURE.md](../ARCHITECTURE.md) (features that cross layers: `docs/features/`, indexed there).

## Code by role

`src/` is split by role, like the entities of the backend (CLAUDE.md rule 8): a role keeps everything about its job in its own folder — its data, its work (calls to the backend, how an error shows, what to re-read), its components and drawing, its pure math. The piano roll is `src/pianoroll/`: the notes it edits (`notes.ts`), its canvas and mouse (`PianoRoll.tsx`, `render.ts`, `pointer.ts`), its rules (`noteFit.ts`, `noteTransforms.ts`, `groove.ts`…). There is no folder of "all the state" or "all the methods".

| Folder | Role |
|---|---|
| [`api/`](src/api/CLAUDE.md) | The contract with the backend (`RpcClient`), dialogs, shell; the TS mirror of the Rust JSON; typed wrappers |
| [`core/`](src/core/CLAUDE.md) | What every role shares: the backend, the project copy, how a call runs and tells the user, the playback tick |
| [`common/`](src/common/CLAUDE.md) | What several roles share: the UI kit (knob, fader, modal, icons, canvas, theme), the look and motion, shared pure helpers (time, tempo map, pitch, math, plot) |
| [`shell/`](src/shell/CLAUDE.md) | The window frame: activity bar, status bar, welcome screen, which tab is open, window keys |
| `selection/` | What is selected: the track, clips, a sample, automation |
| `transport/` | The top strip: playback, position, tempo and meter, the waves |
| `history/` | Undo, redo and the history panel |
| `project/` | Project files, templates, audio export, startup, the project menu |
| [`tracks/`](src/tracks/CLAUDE.md) | The timeline: tracks, folders, clips, markers, takes, bounce |
| `pianoroll/` | The note clip editor: notes, steps, controller lanes, the clip panel |
| `automation/` | Automation lanes, the pattern editor, live writing |
| `instruments/` | The instrument window and a track's MIDI effects |
| `keyboard/` | The computer keyboard as a piano, live notes |
| `recording/` | Note and audio recording, track inputs |
| `midi/` | MIDI keyboards and MIDI learn |
| [`graph/`](src/graph/CLAUDE.md) | The Graph page and the signal path |
| [`effects/`](src/effects/CLAUDE.md) | Effect editors, their live graphs and the nodes' live sound |
| [`mixer/`](src/mixer/CLAUDE.md) | The Mixer page, meters, the master visualizer and meter |
| `stage/` | The virtual stage |
| `samples/` | The project's sounds and the sample editor |
| `sounds/` | The sound browser and putting a sound to work |
| `drums/`, `chords/` | The drum part and chord part dialogs |
| `plugins/`, `presets/` | Third-party plugins and their windows; instrument and effect presets |
| [`settings/`](src/settings/CLAUDE.md) | The Settings page, the preferences and the look made from them |
| [`i18n/`](src/i18n/CLAUDE.md) | Interface languages (English default, Russian) |
| [`mock/`](src/mock/CLAUDE.md) | The browser stand-in for the backend |

Root files: `index.tsx` (start), `composition.ts` (the only place that picks the implementations: Tauri or the mock), `session.tsx` (creates every role once and hands the UI the `Session`), `App.tsx` (the window).

**How a role is built.**
- **Its work** is a factory that takes the shared core and the roles it asks (`createNotesActions(core)`, `createClipsActions(core, views)`) and returns the interface of its job (`NotesActions`). It calls the backend (`core.api`), runs each call through `core.run`/`core.exec` (a failure is a toast and a whole re-read), shows a value at once with `core.patchProject` and selects what it made with `views.showCreated`. Its own state (a clipboard, a scan in progress) lives inside it.
- **Its data** that others read is a factory too (`createLevels`, `createSelection`). A part the UI reads in pieces declares them itself (`ProjectReads`, `PlaybackReads`, `RecordingReads`, `ToastReads`).
- **Its components** take the session with `useSession()` and ask their role: `session.notes.add(…)`, `session.levels`. A component never calls the backend or the core.
- **Its pure math** sits next to it. A module that mirrors a backend rule names its Rust twin (`pianoroll/noteTransforms.ts` ↔ `daw-project/src/note_transforms.rs`), and the mock uses the same module.
- **Another role** is used through what it offers: its factory's interface, its components, its exported functions. Nobody changes another role's state.
- **The session** (`session.tsx`) creates the core and every role once, in order; it is the only place that wires them. A new role is a folder, a factory, a field in `Session` and a line in `createSession`; a new operation: the [add-operation](../.claude/skills/add-operation/SKILL.md) recipe.

**Rules** (checked by Biome: `npm run lint`, `app/biome.json`; choosing implementations only in `composition.ts` is checked in review): no inheritance (classes via `app/lint/no-extends.grit`, interfaces via a grep in the same script); a component (`*.tsx`) imports no `api` client (types are fine), no `core`, no mock and no `composition`; `core` and `api` import no role; `i18n` imports nothing of the app; the mock imports neither the session nor the core; no empty blocks, no unused code.

**Errors.** A failed call is shown by `core.showFailure` (`core/notifications.ts`, used by `run`/`attempt`/`exec` and the project re-read): a backend refusal is only a toast (`daw-rpc` has already logged it); any other failure (the IPC link, a throw in our own code, a failed re-read) also goes to `console.error` → `log/errorReporter.ts` → the log. An error decided by the interface (`core.toast("error", …)`) and silent failures (preview, live notes, `library.home`, the look cache in `settings/appearance.ts`) also go to `console.error`. `console.warn` reaches no log, so failures never use it (the one exception is `settings/log.ts`: a failure to read the log itself would loop back into it). Warnings from opening a project (`project.takeWarnings`: a missing sound…) are logged by the backend; `ProjectActions.showOpenWarnings` only shows them. The reporter is installed before anything else runs (`index.tsx`).

Rust side: `crates/daw-rpc` is the method table (no dependency on Tauri); `app/src-tauri` is a thin root (`src/main.rs`): audio output, autosave, the `rpc` and `rpc_binary` commands, the `tick`/`project` event stream, plugin windows and the AI server.

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd app && npm install
npx tauri dev     # the real app: audio, engine, files
npm run dev       # interface only, in the browser on the mock (handy for layout work)
npm run lint      # Biome: role boundaries, no inheritance, empty blocks (no frontend tests yet)
```
