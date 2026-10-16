# Plan: stop copying the backend into the frontend by hand

Status: **for the owner's decision**. Nothing here is built yet. Measured on `dev` at `b4e3c2a` (2026-10-16).

## The problem

A feature is written in Rust, and then three parts of it are copied into TypeScript by hand:

| Copy | Where | Size today | What every new feature pays |
|---|---|---|---|
| The mock backend | `app/src/mock/` | 45 files, 4,322 lines; plus ~630 lines of mock-only texts in `i18n/en.ts` + `ru.ts` (`catalog`, `mock`, `mockEdits`) and `domain/variation.ts` (used only by the mock) | a handler per operation that repeats the Rust checks and error codes; nothing compares it with the backend |
| The mock catalog (part of the mock) | `mock/catalog.ts` + `i18n` `catalog.*` | ~465 lines | 10–47 lines per new effect (tape 10, transient 14, de-esser 24, dynamic EQ 25, multiband 47), labels written a second and third time; it has already drifted (effect order, param kinds guessed from names) |
| The API types | `app/src/api/types/` (12 files) and `api/api.ts` | ~980 lines of types, ~200 wrappers | each new field or type is copied by hand; `call<T>(method: string, params?: unknown)` is untyped, so nothing checks param names or result types against Rust |

There are three ways to remove the copies. They are independent decisions.

## C. The real backend in the browser instead of the mock (the biggest win)

**What.** When the interface runs in a browser (`npm run dev`), it talks to a real Rust session instead of the TS mock.
- Rust: a dev-only binary `daw-dev` in `daw-app` (`src/dev_main.rs`): a session (`--project`, `--demo`, `--state-dir`,
  `--no-audio`) and a small HTTP server on `127.0.0.1`: `POST /rpc {method, params}` → the result or the `ContractError`
  JSON; `GET /events` — one Server-Sent Events stream with `tick` and `project`. That is all the interface needs: the
  Tauri side today is one `rpc` command plus these two events (`app/src-tauri/src/main.rs`). Host/Origin checks keep it
  local. About 200 lines and a process test.
- Frontend: `api/httpRpc.ts` (~50 lines, `fetch` + `EventSource`), a proxy in `vite.config.ts` (same origin, no CORS),
  `composition.ts`: `insideTauri() ? TauriRpc : HttpRpc`. `MockDialogs` and `MockShell` stay as browser dialogs and
  shell (they don't use the mock backend).
- The existing MCP HTTP endpoint (`daw-mcp/src/http.rs`) can't be reused: it refuses browser requests on purpose (any
  `Origin` header), serves one request at a time, has no events and flattens errors to text.

**Deletes.** `app/src/mock/` (45 files, 4,322 lines), the mock-only `i18n` sections (~630 lines), `domain/variation.ts`,
the mock rules in `biome.json`, and step 7 of the `add-operation` recipe (plus the mock steps of `add-effect`,
`add-instrument`, `add-error-code`). Option A is then not needed.

**Effort:** M. About 4 new files, 10 edited, 46 deleted.

**Costs and risks.**
- UI work in the browser needs the Rust backend built and running; a Rust change means a rebuild and restart (TS hot
  reload is unaffected). A cold build is heavy.
- A debug build may crackle (no `[profile.dev]` tuning): run it with `--release` or add `opt-level` for dependencies.
- Always `cargo run`, never a prebuilt binary. Example of the risk: the configured `daw-headless` MCP server runs a
  `target/release/daw-mcp` from 2026-10-09 that lacks the newer methods (`project_summary`, `analysis_*`, folders, markers).
- A local server that can write files: dev-only, with the Host/Origin checks.
- Lost fake scenarios: fake audio devices (including a broken one), the fake VST3 scan with broken and crashing plugins,
  `?pluginCrash`, `?welcome`. Replacements: a fresh `--state-dir` and the `test-plugins/` fixtures.
- On a machine without a sound device playback may not move (not verified).
- Future frontend tests lose a ready in-memory fake (they are to be written anew anyway).

**Safe path:** two steps. First add `HttpRpc` next to the mock (chosen by `?backend=http`) and use it in Claude's
browser preview for a while; then delete the mock.

**Decision:** is it acceptable that UI work in the browser needs a built Rust backend (and a sound device for playback)?

## B. Generate the TS API types from Rust

**What.** The Rust types already describe the JSON (serde), and 94 of the 127 serde types in `daw-contracts` already
derive `JsonSchema` (schemars 1.2.2 is already a dependency; it already makes the param schemas for `rpc.describe` and MCP).
A test-only printer (~250 lines) turns the schemas into `app/src/api/types/generated.ts` and a method map
`api/rpcMethods.generated.ts` (`method → {params, result}`). `call` becomes `call<M extends keyof RpcMethods>`, so a
wrong param name or result type fails `tsc`. A test fails when the generated files are stale; one command refreshes them.

**Needs.** `JsonSchema` on ~33 more contract types and ~9 RPC result structs; hand-written schemas for `L10n` and
`ContractError`; a Rust `Tick` struct instead of the `json!` in `daw-rpc/src/telemetry.rs`; `RpcTable::add` requires
`R: JsonSchema` (result schemas stay out of `describe`, so MCP is unchanged). The printer must not emit
`interface … extends` (banned by the lint): `flatten` becomes `&`.

**Stays hand-written.** ~15 consts and functions in `api/types`; narrow unions such as `TrackColor` and `Mp3Bitrate`
(unless they become Rust enums); aliases where the TS name differs from Rust (`ExportOptions` → `ExportRequest`,
`GraphNode` → `Node`, …).

**Alternatives.** ts-rs: a new derive dependency on `daw-contracts`, `u64` → `bigint` by default (30 fields), overrides
for `L10n`, base64 and the `try_from` types — not recommended. typeshare can't express our tagged and flattened enums;
specta's value is typed Tauri commands, and we have one `rpc` command.

**Effort:** M–L — the typed method map will surface real mismatches in `api.ts`; that is the point. About 15 files in
contracts, 12 in `daw-rpc`, 8 in the frontend.

**Effect on a feature:** a new field or method shows up as `tsc` errors; nothing to copy. Step 6 of `add-operation`
becomes "regenerate".

**Decisions:** our own schemars printer (no new dependency, ~250 lines we own) or ts-rs (a new dependency on the
contracts)? Should TS type names follow the Rust names (renaming touches ~108 places) or keep aliases?

## A. Generate the mock catalog from Rust (only if the mock stays)

**What.** A test in its own test binary (the label language is a process-wide setting) builds a test session, calls
`catalog.get` in English and in Russian and writes `app/src/mock/catalog.generated.ts`; the same staleness check as B.
`createMockCatalog` shrinks to ~10 lines. The same test can also write the English error templates the mock copies by hand.

**Removes** ~465 hand-written lines; **adds** one generated file (~600–1,000 lines, never edited by hand) and a ~50-line
test. **Effort:** S, about 6 files. **Per new effect:** 10–47 hand lines become one command.

**Decision:** needed only if C is rejected.

## Recommended order

1. **C in two steps** (HttpRpc next to the mock, then delete the mock).
2. **B** — independent of C; start with the typed method map, which finds mismatches in `api.ts` at once.
3. **A** only if C is rejected.
