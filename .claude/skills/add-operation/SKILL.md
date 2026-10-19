---
name: add-operation
description: Recipe for adding a new SazDAW operation end to end — a method on a `*Control` contract, its implementation, an RPC method (which also becomes an MCP tool), the frontend API wrapper, the browser mock handler, an action and the UI. Use when a task adds or changes a user-visible command, an RPC/MCP method or a contract method.
---

# Add an operation (end to end)

Before you start: the map `ARCHITECTURE.md` names the crate and folder; read their `CLAUDE.md` and, if the operation
belongs to a cross-layer feature, its `docs/features/<feature>.md`. Nothing else needs reading.

**Model to copy:** `tracks.setColor` — every layer is a few lines:
`crates/daw-contracts/src/tracks.rs` (`TrackControl::set_color`) → `crates/daw-project/src/tracks.rs` →
`crates/daw-rpc/src/tracks.rs` (`SetColor`, `table.add("tracks.setColor", …)`) → `app/src/api/api.ts` →
`app/src/mock/handlers/tracks.ts` → `app/src/tracks/tracks.ts` → `app/src/tracks/TrackMenu.tsx`.

Every step is checked by the compiler and tests, so no step can be skipped (the mock is the exception: nothing compares
it with the backend, so check it by eye against the Rust implementation).

1. **Contract** — a method in the area's trait (the area file in `crates/daw-contracts/src/`: `tracks.rs`, `clips.rs`,
   `notes.rs`, `graph_control.rs`, `automation.rs`, `history.rs`, `record.rs`, `project_io.rs`…). The doc comment
   describes the errors and edge cases ("the same value creates no undo step", "one undo step"); failures are codes from
   `daw_contracts::codes` — a new one: the `add-error-code` recipe.
2. **Other implementations of the trait** — a new trait method breaks every `impl <Trait> for`: find them with
   `git grep -n "impl <Trait> for"`. Test fakes in other crates get a stub (for `TrackControl`:
   `crates/daw-midi/src/file/fakes.rs`, `crates/daw-audio-recorder/src/tests/mod.rs`, `crates/daw-vst3/tests/support/mod.rs`).
3. **Implementation** — in the crate that owns the area (mostly `crates/daw-project/src/<area>.rs`), through the shared
   edit pipeline: `self.project.edit(&tr!("Undo label", "Подпись отмены"), |project| { … })` gives undo, DSP objects and
   the snapshot. A compound edit is `begin_group` … `end_group`, rollback `cancel_group`. Test: `crates/daw-project/src/tests/<area>.rs`.
4. **Wiring** — only for a new entity or trait: `crates/daw-bootstrap/src/wiring/` and `Controls` (`wiring/mod.rs`).
5. **RPC** — a line in the method table `crates/daw-rpc/src/<area>.rs`:
   `table.add("area.methodName", "What it does.", |c, p: Params| c.area.method(…))`. The params struct is
   `#[derive(Deserialize, JsonSchema)]` with a doc comment on each field. The description and the schema are mandatory:
   the MCP tool for AI is built from them (`daw-mcp` takes every method by itself). Test: `crates/daw-rpc/tests/all/<area>.rs`.
6. **Frontend API** — a typed wrapper in `app/src/api/api.ts` (its group is the RPC namespace); new JSON types in
   `app/src/api/types/<area>.ts` (a mirror of the Rust serde shape, camelCase).
7. **Browser mock** — `h["area.methodName"] = (params) => …` in `app/src/mock/handlers/<area>.ts`, with the same checks
   as the real backend and the same error codes (`app/src/mock/errors.ts`); edits go through `w.edit(label, (p) => …)`,
   the mock's undo labels are `t().mock.undo.*` in `app/src/i18n/en.ts` + `ru.ts`.
8. **The role's work** — a method in the job's interface and its factory in the role's folder (for tracks
   `app/src/tracks/tracks.ts`: `Tracks`, `createTracks(core, …)`): this is where we decide how to show an
   error and what to re-read. An instant display goes through `core.copy.patchProject`, the call through
   `core.calls.exec`/`core.calls.run`; something it creates is selected with `views.showCreated`. A new job of a role goes into
   its `role.ts` (`app/src/tracks/role.ts`); a new role is a folder with a `role.ts` (`defineRole` and its `useX` hook).
9. **Interface** — the role's component takes its role through the hook and calls it: `useTracks().setColor(…)`
   (another job of the role: `useTracks().clips.split(…)`); it sees neither `api` nor `core`. New text
   goes into `app/src/i18n/en.ts` and `ru.ts` (`ru.ts` is typed `Messages`: a missing key fails `tsc`).
10. **Docs** — the `CLAUDE.md` of the area or the feature's `docs/features/<feature>.md`: where it lives, which contract
    it relies on, how to add a similar one. The map `ARCHITECTURE.md` changes only for a new crate, area, feature doc or recipe.

**Check:** `cargo check --workspace`; in `app/`: `npx tsc --noEmit` and `npm run lint`. The full list before a commit
is in `CLAUDE.md` ("How it is checked").
