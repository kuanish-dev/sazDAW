# DAW project rules

These rules matter more than convenience and speed. The map of the code is `ARCHITECTURE.md` (imported at the end): one line per crate, frontend area, feature doc and recipe. How each piece works is in its own doc — `CLAUDE.md` in its crate or folder, `docs/features/` for features that cross layers — so a task reads only the docs of the places it touches.

## Core

1. **The contract comes first.** An interface (a trait in `daw-contracts`, a TS interface on the frontend) describes what an entity does. We design and fix the contract first, then write the implementation. Changing a contract is a deliberate decision, not a side effect of editing an implementation.
2. **Implementations come second.** A concrete implementation is a detail: it must follow the interface and honour the contract (including errors and edge cases); everything else is its own internal business and can be rewritten without consequences for anyone else.
3. **Everything goes through interfaces.** An upper layer sees only the interfaces of the layer below and knows nothing about its implementation. Concrete types are created and wired together in one place: `daw-bootstrap` (Rust) and `src/composition.ts` (frontend).
4. **The upper level delegates to the lower one.** A UI component does not call the backend and does not decide how to show errors: it calls an action (`session.actions.*`), and the action delegates to the API.
5. **Code must be simple.** A simple solution beats a "flexible" one. We don't add abstractions "for the future"; the most obvious code is enough: short functions, single responsibility, clear names, comments only about "why".

6. **Think first, then write.** Before every feature, before the first line of code, answer these questions (briefly, in a message or in the commit description):
   - **Where?** Which layer and crate/folder does it belong to, which interface is affected or is a new one needed?
   - **How?** What is the simplest way that fits the contract? What else has to change (wiring, mock, tests, documentation)?
   - **Does it belong there?** Is an existing type taking on a new role (more than 2–3 roles per type is a reason to stop)? If so, first make a place for it (a new module, type, interface), then add the code. File size by itself is not a criterion: split only by roles.
   - **What already exists?** Is there something ready-made that should be reused rather than duplicated?
   After the feature, ask the same question again: the code lives where people will look for it.
7. **Code is understandable without its author.** The project owner must be able to understand it alone, without an assistant: every crate and every frontend area has a clear name and a short description of its role at the top of the file; a new feature is described next to its code — in its crate's or folder's `CLAUDE.md`, or in `docs/features/<feature>.md` when it crosses layers (where it lives, which contract it relies on, how to add a similar one); the map `ARCHITECTURE.md` gets a line only when a crate, area, feature doc or recipe appears or changes its role; a non-obvious decision is explained by a "why" comment. If understanding requires a chat with the author, the work is unfinished.

8. **Entities are divided by job (role), not by data and actions.** An entity has one clear job, like a guard at the entrance: let in or don't let in, check the pass. Everything needed for that job — data, actions, and references to other entities — it has. Therefore:
   - We don't split into "those who read" and "those who write", and we don't cut along sets of dependencies: a job that needs both to read and to change stays one entity.
   - The test: the job can be named in one phrase without "and" (sample catalogue: "know which sounds the project has"; sample editor: "everything we do with one sound"). If the phrase has "and", it is two entities.
   - A shared core (for example, the project model with its edit pipeline) is shared by all entities through a narrow interface; each takes from it only what its job needs.
   - An interface describes the entity's job as a whole: if an interface mixes two roles, we split the interface by roles, not the implementation into "reading/writing".

9. **Ask it to do the work, don't do it for it.** An entity holds a reference to another (a service, the core, an action) and asks it to do the work: that's normal, the way departments of a company talk to each other. What's bad is walking "into someone else's office" and changing its state behind its back: touching its fields, locks, internal structures, writing to its storage directly. Only the owner changes its state.
   - An entity's internals (fields, locks, internal state) are hidden by a module/scope so the compiler keeps others out of the office (example: the core in `daw-project/src/service/`).
   - The interface (UI) does not change shared session state itself: instant display of a slider value is done by the actions layer (`ctx.patchProject`), not by the component.

10. **No backward compatibility while active development is ongoing.** Project formats, APIs and contracts change without migrations or "transitional" layers; old project files may stop opening. We don't spend code or time on backward compatibility until the owner says development has stabilised.

11. **No error disappears silently.** Failures (backend, interface, panics) go to an on-disk log (`AppLog`, the file `errors.log` in the app state folder; in the interface: "Project" → "Error log") so they can be investigated. New code does not swallow errors without logging them; a failure of the log itself does not crash the app. Contract failures are not free-form text: each is a registered code with parameters (`daw_contracts::ContractError`, registry in `crates/daw-contracts/src/codes/`; see "Errors as codes" in `docs/features/errors.md`).

## Forbidden

- Inheritance (in TS: `class … extends`, `interface … extends`; Rust doesn't have it anyway — don't imitate it with "base" traits that carry implementations). Only composition and implementing interfaces.
- A layer depending on a concrete implementation of another layer (except `daw-bootstrap` and `composition.ts`).
- Allocations and locks in the audio callback.
- `unsafe` anywhere except the `daw-vst3` crate (it is unavoidable there for VST3 C interfaces; every block carries a `// SAFETY: …` comment). `daw-vst3` has its own lint set: the same strict rules but `unsafe_code = "allow"`; guarded by `crates/daw-bootstrap/tests/all/architecture.rs`.
- Editing an implementation so that it stops honouring the contract, for the sake of a quick result.

## How it is checked

- Rust: `crates/daw-bootstrap/tests/all/architecture.rs` — crates depend only on `daw-contracts`, application roots only on `daw-bootstrap`, `daw-rpc`, `daw-mcp` and the contracts; only `daw-vst3` does not inherit the workspace lint rules (`unsafe` is allowed there).
- Frontend: the Biome linter (`npm run lint` in `app/`, config `app/biome.json`): layers as import rules (`ui` doesn't see `api`/`rpc`/dialogs/mocks, `actions` sees only `api` and `domain`, `domain` is pure functions, `api` and `mock` don't know the upper layers), inheritance ban (classes via the plugin `app/lint/no-extends.grit`, interfaces via a `grep` check in the same script), empty blocks (we don't swallow errors), unused code. Implementations are chosen only in `composition.ts` — this is checked by eye.
- Rust is strict: the workspace `Cargo.toml` has `unsafe_code = "forbid"` (except `daw-vst3`), `clippy::pedantic` (without DSP noise: numeric casts etc. are turned off), and bans `unwrap`/`expect`/`panic`/`todo`/`dbg!`/`println!` in production code (allowed in tests via `clippy.toml`). An exception is written as `#[allow(..., reason = "...")]` with a reason, not silently.
- Every implementation has tests for honouring its contract; a new crate/layer without tests is not accepted. Tests are organised by area: `src/tests/<area>.rs` or `tests/<area>.rs` with shared helpers in `tests/common/`.
- Common small things are not duplicated: lock — `daw_contracts::sync::lock`, log entry — `LogEntry::warn`/`error`/`new`, ticks to frames — `Bpm::ticks_per_frame` and `BlockTiming::frame_at`, contract failure — a code from `daw_contracts::codes` (`codes::NAME.with(...)`). A compound edit is `begin_group` … `end_group`; rollback is `cancel_group`, not `undo`.
- Rust formatting — `cargo fmt` (settings in `rustfmt.toml`).
- Before a commit (if the VST3 test fixtures changed or haven't been built — first `scripts/build-test-plugins.sh`): `cargo fmt --all --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and in `app/`: `npx tsc --noEmit`, `npm run lint`, `npx vite build` (there are no frontend tests yet — the owner decided to write them anew later). The VST3 test fixtures `test-plugins/synth` and `test-plugins/distortion` (dev-only, not shipped) are separate workspaces with the same lint rules: if they changed — `scripts/build-test-plugins.sh` (tests, clippy and bundles of both).

## Feature workflow

0. Think it through (rule 6): where, how, is the place getting overgrown. Start from the map below; if the task is one of the usual kinds, follow its recipe (`.claude/skills/`, listed in the map) instead of exploring.
1. Contract (interface + types), 2. contract tests, 3. implementation, 4. wiring in `daw-bootstrap` / `composition.ts`, 5. UI via actions, 6. update the doc of the area or feature (the map only for a new crate, area, feature doc or recipe), 7. check the place again.

## Map

@ARCHITECTURE.md
