---
name: add-instrument
description: Recipe for adding a new built-in instrument to SazDAW — its own crate with an `InstrumentFactory`, the registration in `daw-bootstrap`, the browser mock catalog and labels. Use when a task adds an instrument kind (instruments are built-in crates, not VST3 plugins).
---

# Add a built-in instrument

Before you start: the doc of the closest existing instrument — `crates/daw-synth/CLAUDE.md` (one kind),
`crates/daw-strings/CLAUDE.md` (several kinds on one engine), `crates/daw-drums/`, `crates/daw-sampler/`.

1. **Crate** `crates/daw-<name>/` (a new entity is its own crate):
   - `Cargo.toml` like `crates/daw-synth/Cargo.toml`: depends only on `daw-contracts`, `[lints] workspace = true`;
     a line in `members` of the root `Cargo.toml`;
   - `src/lib.rs` with a `//!` header naming its role; the sound in its own module (`engine.rs` in `daw-synth`);
   - `impl InstrumentFactory` (`crates/daw-contracts/src/instrument.rs`): `kind`, `label` (`L10n { en, ru }`),
     `describe` (the `ParamInfo` table), `create`, and `rebuild_on` for params that need a new instance;
   - `impl Instrument`: it **adds** sound into the buffer; no allocations, locks or I/O in the audio callback.
   - Tests in `src/tests.rs` (like `crates/daw-synth/src/tests.rs`).
2. **Register** in `crates/daw-bootstrap/src/wiring/catalogs.rs` (`instruments.register(daw_<name>::factory())` or
   `register_all(…factories())`) and add the crate to `crates/daw-bootstrap/Cargo.toml`. The engine and the service
   have no branching on the kind name; `crates/daw-bootstrap/tests/all/architecture.rs` checks that the crate depends
   only on the contracts.
3. **Browser mock catalog** — the kind and its params in `instruments` in `app/src/mock/catalog.ts`, labels in
   `catalog.kinds.*` / `catalog.params.*` of `app/src/i18n/en.ts` + `ru.ts`.
4. **Frontend extras, only if needed** — the generic knobs work by themselves; kind-specific behaviour lives in
   `app/src/domain/` (see `synth.ts`, `strings.ts`, `samplerTracks.ts`).
5. **Docs** — `crates/daw-<name>/CLAUDE.md` (role, depends on, how it works), a row in the crates table of the map
   `ARCHITECTURE.md`.

**Check:** `cargo check --workspace`; in `app/`: `npx tsc --noEmit` and `npm run lint`. The full list before a commit
is in `CLAUDE.md` ("How it is checked").
