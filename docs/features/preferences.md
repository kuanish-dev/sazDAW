# Preferences: the user's and the app's

**User preferences (backend).** The role is "remember the user's preferences": the `PreferencesStore` contract (`get`/`set`) and the `Preferences` type (`daw-contracts/src/preferences.rs`; defaults come from `Default`, limits are the `MIN_*`/`MAX_*` constants there; validation and normalization are in `Preferences::checked`, shared by all implementations).

* **Implementation:** `PreferencesFile` (`daw-persistence/src/preferences.rs`, `preferences.json` in the state folder, on top of `JsonState`): no file — defaults; a corrupted file or out-of-range values — defaults and a log entry (`persist`), because the app must start in any case; `set` validates, writes atomically (on failure the previous value stays) and returns what was saved.
* **Readers** get `Arc<dyn PreferencesStore>` in `daw-bootstrap` (it is also `Controls.preferences`): autosave (the interval, `services.rs`), `MidiFiles` (author → Copyright event on MIDI export), the VST3 plugin scanner (the disabled plugins list, `daw-vst3/src/plugins.rs`), and startup: `Startup::open` (`daw-bootstrap/src/lib.rs`) opens the last project only when "Open last project" is on. The backend only stores the appearance settings.
* **RPC:** `preferences.get`, `preferences.set` (`daw-rpc/src/preferences.rs`).
* **A new setting:** a field in `Preferences` (+ a default and validation in `checked`) → a contract test → the reader receives `Arc<dyn PreferencesStore>` in `daw-bootstrap`.

**App preferences (frontend).** The TS type is `api/types/preferences.ts`; defaults, limits and choices are `settings/preferenceRules.ts`, whose limits the mock `mock/handlers/preferences.ts` checks with the backend's codes.

* **State:** the interface copy is a signal in `settings/preferences.ts`, read via `useSettings().preferences.current()`; only `settings/preferences.ts` changes it (`show`, `update`): the new value shows at once, is saved on the backend, and what was saved is shown; a rejection means a toast and a rollback. Loading happens at startup (`app/src/start.ts`) before the first project read, so project dialogs already know the projects folder (`files/project.ts`).
* **Appearance** is applied by `settings/appearance.ts`: theme ("system" follows `prefers-color-scheme` live) and accent are `data-theme`/`data-accent` attributes on the root (accent colors are in `index.css`); `common/theme.ts: palette()` notices their change, and canvases redraw. Theme and accent are cached in `localStorage` only to avoid a flash before the preferences arrive (`applyCachedLook` in `index.tsx`).
* **Zoom** is `AppShell.setZoom` (Tauri: the webview zoom, which needs the permission `core:webview:allow-set-webview-zoom` in `src-tauri/capabilities/default.json`; browser: CSS `zoom`).
* **Track row height** is `useSettings().appearance.rowHeight()` — the default for rows; a track can have its own (see "Track row heights" ([app/src/tracks/CLAUDE.md](../../app/src/tracks/CLAUDE.md))).
