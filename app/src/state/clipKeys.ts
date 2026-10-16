// Timeline clip shortcuts: delete, select, copy and paste — as a table that the help is built from too. With an
// automation clip selected, Delete, ⌘D and Esc act on it instead.
// Arrows (move by a grid step) are not handled here: the grid lives in the timeline panel, not in the session.
import type { Id } from "../api/types";
import type { ShortcutHelp } from "../domain/shortcuts";
import { t, type Messages } from "../i18n";
import type { Session } from "./session";

interface ClipKey {
  keys: string;
  label: keyof Messages["shortcuts"]["clips"];
  codes: string[];
  /** Requires Cmd/Ctrl. */
  meta: boolean;
  /** Works without selected clips too. */
  withoutSelection?: boolean;
  run(session: Session, ids: Id[]): void;
}

const CLIP_KEYS: ClipKey[] = [
  { keys: "⌘A", label: "selectAll", codes: ["KeyA"], meta: true, withoutSelection: true,
    run: (s) => s.selection.selectClips(s.project.clips.map((c) => c.id)) },
  { keys: "⌘V", label: "paste", codes: ["KeyV"], meta: true, withoutSelection: true,
    run: (s) => void s.actions.clips.paste() },
  { keys: "Esc", label: "deselect", codes: ["Escape"], meta: false, run: (s) => s.selection.clear() },
  { keys: "Delete · ⌫", label: "delete", codes: ["Delete", "Backspace"], meta: false,
    run: (s, ids) => void s.actions.clips.removeMany(ids) },
  { keys: "⌘C", label: "copy", codes: ["KeyC"], meta: true, run: (s, ids) => s.actions.clips.remember(ids) },
  { keys: "⌘D", label: "duplicate", codes: ["KeyD"], meta: true, run: (s, ids) => void s.actions.clips.duplicateMany(ids) },
];

/** Clip key help — from the same table. */
export function clipKeysHelp(): readonly ShortcutHelp[] {
  return CLIP_KEYS.map(({ keys, label }) => ({ keys, label: t().shortcuts.clips[label] }));
}

/** Keys on the selected automation clip: delete, duplicate, deselect. */
function handleAutomationClipKey(event: KeyboardEvent, session: Session): boolean {
  const chosen = session.selection.automation();
  if (chosen?.clip == null) return false;
  const { lane, clip } = chosen;
  const meta = event.metaKey || event.ctrlKey;
  const { automation } = session.actions;
  if (event.code === "Delete" || event.code === "Backspace") void automation.removeClip(lane, clip);
  else if (meta && event.code === "KeyD") void automation.duplicateClip(lane, clip);
  else if (event.code === "Escape") automation.select(null);
  else return false;
  event.preventDefault();
  return true;
}

/** Handles a key; `true` — the key was consumed (`preventDefault` already called). */
export function handleClipKey(event: KeyboardEvent, session: Session): boolean {
  if (handleAutomationClipKey(event, session)) return true;
  const ids = session.selection.clips();
  const meta = event.metaKey || event.ctrlKey;
  const key = CLIP_KEYS.find((k) => k.codes.includes(event.code) && (!k.meta || meta) && (k.withoutSelection || ids.length > 0));
  if (!key) return false;
  event.preventDefault();
  key.run(session, ids);
  return true;
}
