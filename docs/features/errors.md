# Errors: the error log and errors as codes

## Error log

One cross-cutting contract `AppLog` (`daw-contracts/src/log.rs`), implementation `FileLog` (`daw-persistence/src/log.rs`): JSON lines in `<state folder>/logs/errors.log`, rotated as it grows, long fields truncated; a write failure does not crash the app.

* **Who writes:** `daw-rpc` logs every rejected call with its code and English message (system and internal kinds, invalid parameters and an unknown method as `error`, other contract rejections as `warn`); panics of all threads (`install_panic_hook` in `daw-bootstrap`, called in the roots); the backend areas, each under its own source tag (`LogEntry::warn`/`error`: `startup`, `dsp`, `samples`, `persist`, `autosave`, `midi`, `ai`…); the interface (`src/log/errorReporter.ts`: uncaught errors, unhandled rejections, `console.error`, `ErrorBoundary`; writes through RPC `log.write`, identical consecutive errors are not repeated).
* **Reading:** RPC `log.recent` / `log.file`, the "Log and about" section of the settings page (`ui/settings/LogSection.tsx`; menu "Project" → "Error log…").

## Errors as codes

Every contract failure is a `daw_contracts::ContractError` (`daw-contracts/src/error.rs`): a registered code plus named parameters, never free text.

* **Code** — `AA-KNN`: `AA` the area prefix, `K` the kind (`ErrorKind`: `0` not found, `1` invalid value, `2` limit or range, `3` state/conflict — not possible right now, `4` system/IO — files, devices, plugins, `5` unsupported, `9` internal), `NN` the number within the area and kind. One code — one situation. Example: `TR-001` "Track {id} not found".
* **Registry** — `crates/daw-contracts/src/codes/`: one module per area, each a `codes! { … }` list of names, codes and English templates; the table of prefixes is at the top of `codes/mod.rs`. The English message is the template filled with the parameters (`ContractError::message`).
* **Building an error** — `codes::tracks::NOT_FOUND.with("id", id)`, or `.err()` without parameters; check a specific failure with `error.is(codes::…)`, its kind with `error.kind()`.
* **Over the wire** — RPC and Tauri send `{ code, params, message }`. The interface (`app/src/i18n/errors/`) shows the text in the current language: English is the backend's `message`, Russian is the template from `errors/ru.ts` looked up by code and filled with `params`; the code is appended to the text, so a screenshot names the exact failure. MCP clients get `CODE: message` (`daw-mcp/src/lib.rs`). The error log records the code and message as well.
* **Test** — `codes/mod.rs` checks that codes are unique, well-formed (prefix and kind digit match their area module) and that every code has a Russian translation in `app/src/i18n/errors/ru.ts`.
* **How to add a failure:** the recipe [add-error-code](../../.claude/skills/add-error-code/SKILL.md) (registry line, raising it, the Russian text, the mock, a new area).
