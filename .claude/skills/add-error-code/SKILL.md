---
name: add-error-code
description: Recipe for adding a new contract failure to SazDAW — a registered error code with parameters, its Russian translation, and the same code in the browser mock. Use when code needs to report a new failure from a contract method (failures are never free text).
---

# Add an error code

The rules are in "Errors as codes" (`docs/features/errors.md`): a code is `AA-KNN` — the area prefix, the kind digit
(`0` not found, `1` invalid value, `2` limit or range, `3` state/conflict, `4` system/IO, `5` unsupported, `9`
internal) and the number within the area and kind.

1. **Register** a line in the area module `crates/daw-contracts/src/codes/<area>.rs`, inside `codes! { … }`:
   `NAME = "AA-KNN", "English template with {param}";` — the next free number for the kind; one code, one situation.
   A new area: a new module, its prefix in the table at the top of `codes/mod.rs` and in `AREAS`.
2. **Raise it** with `codes::<area>::NAME.with("param", value)` (more params: chained `.with(…)`; none: `.err()`).
   Tests check a specific failure with `error.is(codes::…)`.
3. **Russian text** with the same `{param}` placeholders in `app/src/i18n/errors/ru.ts` — the test in `codes/mod.rs`
   fails without it (and on a duplicate or malformed code).
4. **Browser mock** — if the mock reproduces this failure, return the same code there (`app/src/mock/errors.ts`).
5. Describe the failure in the contract method's doc comment.

**Check:** `cargo check --workspace`; in `app/`: `npx tsc --noEmit`. The full list before a commit is in `CLAUDE.md`.
