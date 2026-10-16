---
name: add-effect
description: Recipe for adding a new built-in audio effect to SazDAW — a `daw-effects` module, its line in `factories()`, the browser mock catalog and labels, and its live graph in the effect editor. Use when a task adds a new effect kind or a new built-in effect (effects are built-in crates, not VST3 plugins).
---

# Add a built-in effect

Before you start: `crates/daw-effects/CLAUDE.md` ("Built-in effects": what exists and how each is built) and, for the
graph, `app/src/ui/effects/CLAUDE.md` ("Live effect visuals"). Nothing else needs reading.

**Model to copy:** commit `7a591ae`, the transient shaper (`git show 7a591ae`) — 10 files, the whole path. A kind that
reuses an existing graph: `b4e3c2a` (tape, drawn by `ShaperGraph`).

1. **Module** `crates/daw-effects/src/<name>.rs`:
   - a `//!` header: what it does and how; if it has a graph, which frontend module mirrors it ("a change here must be
     mirrored there");
   - `const PARAMS: [ParamInfo; N]` built with `number(…)` (and the choice helpers), labels as `L10n { en, ru }` — the
     real app shows these labels;
   - `impl EffectKind` (`KIND`, `LABEL`, `PARAMS`) and `impl Effect` (`set_param` through `clamp_param`, `prepare`,
     `reset`, `process` with no allocations, locks or I/O). Latency, if any, is reported (see "Plugin delay
     compensation" in `docs/features/pdc.md`). There is no `rebuild_on` for effects: every param changes through
     `set_param`, so whatever a param's range needs is allocated in `new`/`prepare`.
   - Reuse the shared DSP blocks: `biquad.rs`, `smooth.rs`, `dynamics.rs` (`Follower`), `crossover.rs`, `oversample.rs`, `fft.rs`.
2. **Register** in `crates/daw-effects/src/lib.rs`: `mod <name>;` and `factory::<<name>::Type>()` in `factories()`.
   That is all on the Rust side: the engine and the service have no branching on the kind name.
3. **Tests** in the module: response or behaviour, finite and bounded output, no steps when params change between
   blocks (`testing.rs: max_step`), harmonics (`testing.rs: tone`), latency (`testing.rs: assert_latency_at_impulse`).
   The shared contract check (`testing.rs: assert_honours_contract`) runs on every kind in `factories()` by itself.
4. **Browser mock catalog** — the same kind with the same params (name, range, default, choices via `choice`) in
   `app/src/mock/catalog.ts`, and their labels in `app/src/i18n/en.ts` + `ru.ts`: `catalog.kinds.<kind>`,
   `catalog.params.<param>` (only the mock reads these; they repeat the Rust `L10n` labels).
5. **Graph** (every built-in effect has one): a pure mirror of the Rust math in `app/src/domain/<name>Curve.ts` (settings
   by param name through `catalog.ts: namedParams`), a component `app/src/ui/effects/<Name>Graph.tsx` (`listenToNode`
   once if it shows the live sound), a `Match` and the kind in `OWN_GRAPHS` in `app/src/ui/effects/EffectGraph.tsx`
   (a kind drawn by an existing graph goes into its table there instead: `DYNAMICS` or `SHAPERS`),
   texts in `graph.effect.*` in `en.ts` + `ru.ts`. The generic knobs appear by themselves (`ui/common/ParamControl.tsx`).
6. **Docs** — the effect's paragraph in "Built-in effects" and its name in the role line of `crates/daw-effects/CLAUDE.md`;
   its bullet in "Live effect visuals" in `app/src/ui/effects/CLAUDE.md`. The map needs no change.

**Check:** `cargo check --workspace`; in `app/`: `npx tsc --noEmit` and `npm run lint`. The full list before a commit
is in `CLAUDE.md` ("How it is checked").
