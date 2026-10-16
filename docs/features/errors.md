# Errors: the error log and errors as codes

## Error log

One cross-cutting contract `AppLog` (`daw-contracts/src/log.rs`), implementation `FileLog` (`daw-persistence`): JSON lines in `<state folder>/logs/errors.log`,
rotated every 2 MB (3 old files are kept), long fields are truncated, a write failure does not crash the app. Who writes: `daw-rpc` (every rejected call with its code and English message: system and internal kinds —
`error`, other contract rejections — `warn`, invalid parameters and unknown method — `error`), panics of all threads (`install_panic_hook` in `daw-bootstrap`, called in the roots),
startup and audio (`startup`, `audio`), DSP object reconciliation (`dsp`), truncated audio files (`samples`), state files and project saving (`persist`), autosave (`autosave`), MIDI (`midi`), built-in patterns (`patterns`), the AI server (`ai`), the interface (`src/log/errorReporter.ts`: `window.onerror`, unhandled promise rejections, `console.error`, `ErrorBoundary`; writes through RPC `log.write`,
identical consecutive errors are not repeated). Reading: RPC `log.recent` / `log.file`, the "Log and about" section of the settings page (`ui/settings/LogSection.tsx`; menu "Project" → "Error log…"), "Copy all".

## Errors as codes

Every contract failure is a `daw_contracts::ContractError` (`daw-contracts/src/error.rs`): a registered code plus named parameters, never free text.

* **Code** — `AA-KNN`: `AA` the area prefix, `K` the kind (`ErrorKind`: `0` not found, `1` invalid value, `2` limit or range, `3` state/conflict — not possible right now, `4` system/IO — files, devices, plugins, `5` unsupported, `9` internal), `NN` the number within the area and kind. One code — one situation. Example: `TR-001` "Track {id} not found".
* **Registry** — `crates/daw-contracts/src/codes/`: one module per area (`tracks.rs`, `clips.rs`, `graph.rs`…), each a `codes! { NAME = "AA-KNN", "English template with {param}"; … }` list; the table of prefixes is at the top of `codes/mod.rs`. The English message is the template filled with the parameters (`ContractError::message`).
* **Building an error** — `codes::tracks::NOT_FOUND.with("id", id)` (more parameters — chained `.with(...)`), or `.err()` when there are none; check a specific failure with `error.is(codes::…)`, its kind with `error.kind()`.
* **Over the wire** — RPC and Tauri send `{ code, params, message }`. The interface (`app/src/i18n/errors/`) shows the text in the current language: English is the backend's `message`, Russian is the template from `errors/ru.ts` looked up by code and filled with `params`; the code is appended to the text, so a screenshot names the exact failure. MCP clients get `CODE: message` (for example `TR-001: Track 7 not found`). The error log records the code and message as well.
* **Test** — `codes/mod.rs` checks that codes are unique, well-formed (prefix and kind digit match their area module) and that every code has a Russian translation in `app/src/i18n/errors/ru.ts`.
* **How to add a failure:** add a line to the area module (next free number for the kind), build it with `codes::NAME.with("param", v)`, add the Russian text with the same `{param}` placeholders to `app/src/i18n/errors/ru.ts`. If the browser mock (`app/src/mock/`) must reproduce it, return the same code there. A new area: a new module, a prefix in the table and in `AREAS` in `codes/mod.rs`.
