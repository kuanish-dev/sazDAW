# `src/shell/` — the window frame

The window itself (`AppWindow.tsx`: its layout and the window-wide listeners — keys, the computer keyboard piano, the release of live-written controls), the activity bar and the status bar, the hover hint (`HintText`), the welcome screen and the loading overlay, and what the window shows (`views.ts`: the open tab, the settings section, the instrument window, and `showCreated`, which selects what a role just made once the project copy has it).

Map of the whole code: [ARCHITECTURE.md](../../../ARCHITECTURE.md).

**Window keys** are the table in `shell/globalKeys.ts` (`installGlobalKeys`); the clip keys of the Tracks page are `tracks/clipKeys.ts`. The three ways a part of the window keeps keys from them (`data-owns-space`, `data-owns-keys`, `claimSpace`) are listed in the header of `globalKeys.ts`; a new panel with its own keys picks one of them. Each clipboard lives in its role: notes in `pianoroll/notes.ts` (`copy`/`canPaste`/`paste`), automation points in `automation/automation.ts` (`copyPoints`/`pastedPoints`), clips in `tracks/clips.ts`.

**Keyboard shortcut help** (Settings → `settings/ShortcutsSection.tsx`) is built from the same tables the keys work from (`globalKeysHelp`, `clipKeysHelp`, `PIANO_ROLL_KEYS_HELP`, `keyboardPianoHelp`, `GRAPH_KEYS_HELP`, `PATTERN_KEYS_HELP`; the shared row type and key labels are in `common/shortcuts.ts`). A new key is a row in the table next to its handler; the help shows it automatically.
