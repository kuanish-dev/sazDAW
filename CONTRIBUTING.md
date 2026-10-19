# Contributing

Thank you for wanting to help. This file explains the code rules and the workflow for people. The strict list of
rules is in [CLAUDE.md](CLAUDE.md) (it is also the instruction file for AI assistants); the map of layers and areas
is [ARCHITECTURE.md](ARCHITECTURE.md), and each crate and frontend folder describes itself in its own `CLAUDE.md`
(features that cross layers — in `docs/features/`). The project language is English: code, comments, documentation and commit
messages. The app interface is bilingual: English (default) and Russian — every new interface text needs both.

The project is in active development. **We don't maintain backward compatibility yet**: the project format, APIs and
contracts change without migrations or "transitional" layers, and old project files may stop opening. Don't spend
code on it.

## Core principles

### Contract first, then implementation

An interface (a trait in `crates/daw-contracts` or a TS interface on the frontend) describes *what* an entity does,
including errors and edge cases. It is designed and fixed first. The implementation is a detail: it must honour the
contract, and everything else is its own internal business. A contract may change, but as a deliberate decision, not
as a side effect of editing an implementation. Conversely, you must not "tweak" an implementation so that it stops
honouring the contract, for the sake of a quick result.

### Layers see only interfaces

An upper layer knows only the interfaces of the layer below. Concrete types are created and wired together in one
place: `crates/daw-bootstrap` (Rust) and `app/src/composition.ts` (frontend). Implementation crates depend only on
`daw-contracts` and don't know about each other.

On the frontend, a UI component doesn't call the backend and doesn't decide how to show errors: it asks its role
(`use<Role>()`), and the role calls the API. Even the instant display of a slider value is done by the role,
not by the component. The frontend is split by role like the backend: a role keeps its data, its work, its components
and its math in one folder (see [app/CLAUDE.md](app/CLAUDE.md)).

### Entities are divided by role

Each entity has one job that can be named in one phrase without "and": "sample catalogue — know which sounds the
project has", "sample editor — everything we do with one sound". For that job the entity has everything it needs —
both data and actions. That's why we **don't** split entities into "readers" and "writers" and don't cut them along
sets of dependencies. If an interface mixes two roles, we split the interface by roles.

File size by itself is not a reason to split a file: we split only by roles.

### Ask, don't do it for someone else

An entity may hold a reference to another one and ask it to do work. But changing someone else's state behind its
owner's back is not allowed: we don't touch other entities' fields, locks or internal structures. An entity's
internals are hidden by a module or scope so the compiler itself keeps others out of "someone else's office"
(example: the core in `crates/daw-project/src/service/`).

### Simple, not "flexible"

The most obvious solution beats a universal one. No abstractions "for the future". Short functions, clear names,
comments only about "why". Every crate and every frontend area has a short description of its role at the top of the
file. The code must be understandable without its author.

### Errors never disappear silently

Every failure — backend, interface, panic — goes to an on-disk log (`AppLog`, the file `errors.log` in the state
folder; in the interface: "Project" → "Error log"). New code doesn't swallow errors without logging them. A failure
of the log itself doesn't crash the app. Entries are written with `LogEntry::warn` / `error` / `new`.

Contract failures are registered error codes, not free text: `daw_contracts::ContractError` built from
`daw_contracts::codes` (for example `codes::tracks::NOT_FOUND.with("id", id)`). A new failure gets a new line in its
area module in `crates/daw-contracts/src/codes/` and a Russian translation in `app/src/i18n/errors/ru.ts`. Details —
"Errors as codes" in [docs/features/errors.md](docs/features/errors.md).

## Forbidden

- **Inheritance.** In TS — `class … extends` and `interface … extends`; in Rust we don't imitate it with "base"
  traits that carry implementations. Only composition and implementing interfaces.
- **A layer depending on a concrete implementation** of another layer (except `daw-bootstrap` and `composition.ts`).
- **Allocations, locks and I/O in the audio callback.** Engine tables are allocated in advance; exchange with the
  audio thread goes through atomics, ring queues and immutable snapshots.
- **`unsafe`** anywhere except `crates/daw-vst3` (it can't be avoided there because of the VST3 C interfaces). Every
  `unsafe` block has a `// SAFETY: …` comment.
- **`unwrap` / `expect` / `panic!` / `todo!` / `dbg!` / `println!`** in production code (allowed in tests). If an
  exception is really needed — `#[allow(..., reason = "...")]` with a reason.

Much of this is checked automatically: the architecture test in `crates/daw-bootstrap` watches crate dependencies and
lint rules, `clippy::pedantic` and the bans are in the workspace `Cargo.toml`, frontend layers and the inheritance ban
are enforced by the Biome linter (`app/biome.json`, plugin `app/lint/no-extends.grit`).

Common small things are not duplicated: lock — `daw_contracts::sync::lock`, ticks to frames — `Bpm::ticks_per_frame`
and `BlockTiming::frame_at`, contract failure — a code from `daw_contracts::codes`, compound edit — `begin_group` …
`end_group`, rollback — `cancel_group` (not `undo`).

## Feature workflow

0. **Think before the first line of code** and briefly write down the answers (in the PR or commit description):
   - *Where?* Which layer, crate or folder; which interface is affected or is a new one needed.
   - *How?* The simplest way within the contract; what else has to change (wiring, mock, tests, documentation).
   - *Does it belong there?* Is an existing type getting a new role (more than 2–3 roles is a reason to stop and make
     a new place)?
   - *What already exists?* Is there something ready-made worth reusing.
1. **Contract** — interface and types.
2. **Contract tests.**
3. **Implementation.**
4. **Wiring** in `daw-bootstrap` / `composition.ts`.
5. **UI** via its role (`use<Role>()`).
6. **Update the docs:** the `CLAUDE.md` of the area (or `docs/features/<feature>.md` for a feature that crosses layers):
   where it lives, which contract it relies on, how to add a similar one. [ARCHITECTURE.md](ARCHITECTURE.md), the map, gets a
   line only for a new crate, area, feature doc or recipe.
7. **Check the place again:** the code lives where people will look for it.

### Common tasks

Step-by-step recipes are in `.claude/skills/` (Claude Code uses them as skills; they are plain Markdown for people
too) — we don't duplicate them here:

- **New operation / RPC method** (contract → implementation → `daw-rpc` → frontend API → mock → action → component) —
  [add-operation](.claude/skills/add-operation/SKILL.md). A method description and a parameter schema are mandatory:
  the MCP tool for AI is built from them.
- **New error** — [add-error-code](.claude/skills/add-error-code/SKILL.md).
- **New effect** — [add-effect](.claude/skills/add-effect/SKILL.md) (a module with parameters, a line in the factories,
  the same kind in the frontend mock catalog).
- **New instrument** — [add-instrument](.claude/skills/add-instrument/SKILL.md); the engine and the service have no
  branching on the kind name.
- **MIDI effect** — "How to add a similar one" in [crates/daw-note-effects/CLAUDE.md](crates/daw-note-effects/CLAUDE.md).
- **Another plugin format** — "How to add a similar one" in [crates/daw-vst3/CLAUDE.md](crates/daw-vst3/CLAUDE.md).

## Tests

- Every implementation has tests for honouring its contract; a new crate or layer without tests is not accepted.
- Tests are organised by area: `src/tests/<area>.rs` inside the crate or `tests/<area>.rs`, shared helpers in
  `tests/common/`. Put a new test into the file of its area, not into a "common pile".
- Tests that need a VST3 plugin (the dev-only fixtures in `test-plugins/`, built by `scripts/build-test-plugins.sh`,
  or an installed one like Surge XT) are skipped with a clear message when it is missing, rather than failing.
- There are no frontend tests yet — it was decided to write them anew later. The browser backend mock
  (`app/src/mock/`) must repeat the checks and error codes of the real backend.

## Checks before a PR

CI runs the same checks, but run them locally too:

```bash
# the VST3 fixtures for the host tests (otherwise those tests are skipped)
scripts/build-test-plugins.sh

cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd app
npx tsc --noEmit
npm run lint
npx vite build
```

The test plugins `test-plugins/synth` and `test-plugins/distortion` are separate workspaces with the same lint rules:
`scripts/build-test-plugins.sh` runs their tests and clippy and builds their bundles. They are dev-only fixtures for
the VST3 host tests: the app neither ships nor loads them. New instruments and effects are built into SazDAW (Rust
crates), never written as plugins.

Rust formatting — `cargo fmt` (settings in `rustfmt.toml`).

## Dependencies and licenses

SazDAW and the test plugins in `test-plugins/` (not distributed) are both under GPL-3.0-or-later. The list of all third-party packages
is `THIRD_PARTY_NOTICES.md`, generated by a script. If you added, removed or updated a dependency (Rust or npm):

```bash
python3 scripts/third_party_notices.py          # regenerate THIRD_PARTY_NOTICES.md
python3 scripts/third_party_notices.py --check  # check that the list is up to date
```

The script reads `cargo metadata` of the root workspace and `app/node_modules`, so run `npm install` in `app/`
first.

If `--check` reports an unknown license, don't silently add it to the allowed list: describe in the PR what the
license is and whether the package may be distributed together with SazDAW. More in
[docs/licenses.md](docs/licenses.md).

By submitting changes you agree that they are distributed under the repository's license, GPL-3.0-or-later. A new
dependency must be compatible with it: permissive licenses (MIT, Apache-2.0, BSD…), MPL-2.0 and (L)GPL-3.0 are.

## Commit messages

In English, descriptive. The first line says what changed in terms of features, separated by semicolons if there are
several changes. Then a list of details: what was done and where, and why it was done this way. Examples:

```
Plugin crash protection, delay compensation, fitting audio to tempo and crossfades
Mixing: filter, gate, saturator, delay in beats; pre/post-fader sends; "Mixer" tab
Code simplification by the rules: removed extra abstractions, duplicates and repeated tests
```

## Pull request

- Branches: all work goes to **`dev`**. The owner commits straight to `dev`; an outside contribution is a branch
  from the latest `dev` with a PR into `dev`. `dev` is merged into `main` when it is stable; `main` gets no direct
  work.
- One PR — one clear topic.
- In the description: what and why, answers to "where / how / does it belong there / what already exists", how it
  was tested.
- All checks above pass; new code is covered by contract tests.
- If a contract changed — say so explicitly and why.
- The doc of the changed area or feature is updated; the map `ARCHITECTURE.md` if a crate, area, feature doc or recipe
  appeared.
- If dependencies were added — `THIRD_PARTY_NOTICES.md` is updated.
- Interface changes — with a screenshot where possible.
