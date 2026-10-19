// Window-wide shortcuts: transport, marker jumps, history, project files and (on the Tracks page) timeline clips.
// Three ways a part of the window keeps keys from them, and only these three:
// - a focused text field, or a list marked `data-owns-space` (the sound browser replays a sound on Space), keeps its keys;
// - a focused panel marked `data-owns-keys` (piano roll, automation pattern) swallows what it handled (`preventDefault`):
//   then clip keys don't reach the timeline, while the common ones (space, ⌘Z, ⌘S) work over it too;
// - a view that gives Space its own meaning while it is shown, whatever has focus (the sample editor), `claimSpace`s it.
import { codeLabel, type ShortcutHelp } from "../common/shortcuts";
import { t, type Messages } from "../i18n";
import type { FilesRole } from "../files/role";
import type { HistoryActions } from "../history/history";
import { type ClipKeyRoles, handleClipKey } from "../tracks/clipKeys";
import type { TransportRole } from "../transport/role";
import type { Views } from "./views";

/** The roles the window keys ask: the clip keys' and the transport, history, files and what the window shows. */
export type WindowKeyRoles = ClipKeyRoles & {
  readonly transport: TransportRole;
  readonly history: HistoryActions;
  readonly files: FilesRole;
  readonly shell: Views;
};

interface Shortcut {
  /** Caption for the help (Settings → Keyboard shortcuts); a function when it depends on the language. */
  keys: string | (() => string);
  label: keyof Messages["shortcuts"]["global"];
  code: string;
  /** Requires Cmd/Ctrl. */
  meta: boolean;
  /** Requires Alt/Option. */
  alt?: boolean;
  /** Works without an open project too (welcome screen). */
  withoutProject?: boolean;
  run(roles: WindowKeyRoles, event: KeyboardEvent): Promise<void>;
}

const SHORTCUTS: readonly Shortcut[] = [
  { keys: () => codeLabel("Space"), label: "playPause", code: "Space", meta: false, run: (s) => s.transport.togglePlay() },
  { keys: "⌘Z · ⇧⌘Z", label: "undoRedo", code: "KeyZ", meta: true, run: (s, e) => (e.shiftKey ? s.history.redo() : s.history.undo()) },
  { keys: "⌘Y", label: "redo", code: "KeyY", meta: true, run: (s) => s.history.redo() },
  { keys: "⌘S · ⇧⌘S", label: "save", code: "KeyS", meta: true, run: (s, e) => (e.shiftKey ? s.files.saveProjectAs() : s.files.saveProject()) },
  { keys: "⌘O", label: "open", code: "KeyO", meta: true, withoutProject: true, run: (s) => s.files.openProject() },
  { keys: "⌘N", label: "newProject", code: "KeyN", meta: true, withoutProject: true, run: (s) => s.files.newProject() },
  { keys: "⌥←", label: "previousMarker", code: "ArrowLeft", meta: false, alt: true, run: (s) => s.tracks.markers.jump(-1) },
  { keys: "⌥→", label: "nextMarker", code: "ArrowRight", meta: false, alt: true, run: (s) => s.tracks.markers.jump(1) },
] as const;

/** Window key help — from the same table the keys work by. */
export function globalKeysHelp(): readonly ShortcutHelp[] {
  return SHORTCUTS.map(({ keys, label }) => ({ keys: typeof keys === "string" ? keys : keys(), label: t().shortcuts.global[label] }));
}

/** Text inputs and the sound browser list (it replays the sound on space itself) keep keys to themselves. */
const OWN_TEXT_KEYS = "input, select, textarea, [data-owns-space]";
const OWN_PANEL_KEYS = "[data-owns-keys]";

/** Installs the keys on the window; returns the uninstall function. */
export function installGlobalKeys(roles: WindowKeyRoles): () => void {
  const onKey = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement;
    if (event.defaultPrevented || target.closest(OWN_TEXT_KEYS)) return;
    const meta = event.metaKey || event.ctrlKey;
    // Space works with any modifiers, the rest only with Cmd/Ctrl.
    const shortcut = SHORTCUTS.find((s) => s.code === event.code && (s.meta ? meta : true) && (s.alt ? event.altKey : true));
    if (!roles.copy.hasProject() && !shortcut?.withoutProject) return;
    if (shortcut) {
      event.preventDefault();
      void shortcut.run(roles, event);
    } else if (roles.shell.tab() === "tracks" && !target.closest(OWN_PANEL_KEYS)) {
      handleClipKey(event, roles);
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}

/**
 * Space for a shown view instead of the transport, whatever has focus (text fields keep it): a capture-phase listener
 * runs before the window keys and stops them. Returns the release function.
 */
export function claimSpace(onSpace: () => void): () => void {
  const onKey = (event: KeyboardEvent) => {
    if (event.code !== "Space" || (event.target as HTMLElement).closest("input, select, textarea")) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    // Blur the focused button, otherwise releasing Space would "press" it again.
    (document.activeElement as HTMLElement | null)?.blur?.();
    onSpace();
  };
  window.addEventListener("keydown", onKey, true);
  return () => window.removeEventListener("keydown", onKey, true);
}
