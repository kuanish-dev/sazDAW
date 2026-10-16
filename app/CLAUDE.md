# Frontend (`app/`: Solid.js + TypeScript + Tailwind v4, Tauri 2 shell)

The interface talks to the backend through a single contract, `RpcClient` (`src/api/rpc.ts`): `call(method, params)` plus two subscriptions —
`tick` (position, meters, load, ~30 times per second) and `project` (the project changed → re-read it in full).

Map of the whole code: [ARCHITECTURE.md](../ARCHITECTURE.md) (features that cross layers: `docs/features/`, indexed there). Each layer below has its own doc, and so do [`src/ui/effects/`](src/ui/effects/CLAUDE.md), [`src/ui/graph/`](src/ui/graph/CLAUDE.md), [`src/ui/tracks/`](src/ui/tracks/CLAUDE.md), [`src/ui/mixer/`](src/ui/mixer/CLAUDE.md), [`src/ui/settings/`](src/ui/settings/CLAUDE.md), [`src/i18n/`](src/i18n/CLAUDE.md) and [`src/mock/`](src/mock/CLAUDE.md).

| Layer | Folder | Knows about |
|---|---|---|
| Communication contract and model | `src/api/` — [src/api/CLAUDE.md](src/api/CLAUDE.md) | — |
| Domain logic | `src/domain/` — [src/domain/CLAUDE.md](src/domain/CLAUDE.md) | only the `api/types` types |
| Actions | `src/actions/` — [src/actions/CLAUDE.md](src/actions/CLAUDE.md) | `api`, `domain`, the roles in `actions/context.ts` |
| State | `src/state/` — [src/state/CLAUDE.md](src/state/CLAUDE.md) | `RpcClient`, dialogs, actions, domain |
| UI | `src/ui/*` — [src/ui/CLAUDE.md](src/ui/CLAUDE.md) | state, actions, domain |
| Communication implementations | `src/api/tauriRpc.ts`, `src/api/tauriDialogs.ts`, `src/api/tauriShell.ts` (Tauri), `src/mock/MockRpc.ts`, `src/mock/MockDialogs.ts`, `src/mock/MockShell.ts` (browser) | the contract |
| Composition root | `src/composition.ts` | chooses the implementation |

Rules (checked by the Biome linter: `npm run lint`, `app/biome.json`; choosing implementations only in `composition.ts` is checked in review): no inheritance; `ui` does not see `api`/`rpc`/dialogs/mocks and only goes through `session.actions`; each layer knows only the layer below; implementations (`TauriRpc`, `MockRpc`, dialogs) are chosen only in `composition.ts`. Errors: a failed call is shown by `ctx.showFailure` (`state/notifications.ts`, used by `run`/`attempt`/`exec` and the project re-read): a backend refusal is only a toast (`daw-rpc` has written it to the log), any other failure (the IPC link, a throw in our own code, a failed re-read) also goes to `console.error` → `log/errorReporter.ts` → the log; an error decided by the interface (`ctx.toast("error", …)`) also goes to `console.error`; silent failures (preview, live notes, `library.home`, the look cache in `state/appearance.ts`) also go to `console.error`. `console.warn` reaches no log, so failures never use it (the one exception is `actions/log.ts`: a failure to read the log itself would loop back into it). Warnings from opening a project (`project.takeWarnings`: a missing sound…) are backend errors the backend logs; `ProjectActions.showOpenWarnings` only shows them, after open, after restoring an autosave and once at startup. The reporter is installed before anything else runs (`index.tsx`). The backend counterpart is `crates/daw-bootstrap/tests/architecture.rs`: layers depend only on `daw-contracts`, application roots only on `daw-bootstrap` and `daw-rpc`.

Rust side: `crates/daw-rpc` is a method table "name → adapter over a contract" with no dependency on Tauri
(JSON in and out, its own tests); `app/src-tauri` is a thin root: audio output, autosave,
the `rpc` command, the `tick`/`project` event stream.

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cd app && npm install
npx tauri dev     # the real app: audio, engine, files
npm run dev       # interface only, in the browser on the mock (handy for layout work)
npm run lint      # Biome: layers, no inheritance, empty blocks (no frontend tests yet)
```
