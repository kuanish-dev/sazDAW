import type { Notes } from "./notes";
import type { Id, NewNote, Note } from "../api/types";
import { copyNotes, duplicateAt, moveNotes, pasteNotes, type NoteChange } from "./noteSelection";
import type { ShortcutHelp } from "../common/shortcuts";
import { t } from "../i18n";

const SEMITONE = 1;
const OCTAVE = 12;

type NoteKeyAction = "deselect" | "remove" | "selectAll" | "copy" | "cut" | "paste" | "duplicate" | "quantize" | "nudge";

interface NoteKey {
  keys: string;
  action: NoteKeyAction;
  matches(event: KeyboardEvent, meta: boolean): boolean;
}

/** Piano roll panel keys: they work by this table, and the help is built from it too. */
const NOTE_KEYS: readonly NoteKey[] = [
  { keys: "Esc", action: "deselect", matches: (e) => e.key === "Escape" },
  { keys: "Delete · ⌫", action: "remove", matches: (e) => e.key === "Delete" || e.key === "Backspace" },
  { keys: "⌘A", action: "selectAll", matches: (e, meta) => meta && e.code === "KeyA" },
  { keys: "⌘C", action: "copy", matches: (e, meta) => meta && e.code === "KeyC" },
  { keys: "⌘X", action: "cut", matches: (e, meta) => meta && e.code === "KeyX" },
  { keys: "⌘V", action: "paste", matches: (e, meta) => meta && e.code === "KeyV" },
  { keys: "⌘D", action: "duplicate", matches: (e, meta) => meta && e.code === "KeyD" },
  { keys: "⌥Q", action: "quantize", matches: (e) => e.altKey && e.code === "KeyQ" },
  { keys: "← → ↑ ↓", action: "nudge", matches: (e) => e.key.startsWith("Arrow") },
] as const;

/** `label` is a getter so the help follows the interface language. */
export const PIANO_ROLL_KEYS_HELP: readonly ShortcutHelp[] = NOTE_KEYS.map(({ keys, action }) => ({
  keys,
  get label() {
    return t().pianoRoll.keys[action];
  },
}));

export interface NoteEditingDeps {
  notes: Notes;
  /** The clip being edited; `null` if no clip is selected. */
  target(): Id | null;
  noteList(): Note[];
  selectedNotes(): Note[];
  selected(): Set<number>;
  setSelected(ids: Set<number>): void;
  /** Step for grid operations (quantize, duplicate). */
  quantizeStep(): number;
  /** Left/right arrow step. */
  gridStep(): number;
  /** The clip's bar (Shift + arrow): by the time signature at the clip start. */
  barTicks(): number;
  /** Where to paste: the last pointer position over the grid. */
  pasteAt(): number;
}

export function createNoteEditing(deps: NoteEditingDeps) {
  const applyChanges = async (changes: NoteChange[]) => {
    const clip = deps.target();
    if (clip !== null && changes.length > 0) await deps.notes.updateNotes(clip, changes);
  };

  const removeSelected = async () => {
    const clip = deps.target();
    const ids = [...deps.selected()];
    if (clip === null || ids.length === 0) return;
    deps.setSelected(new Set<number>());
    await deps.notes.removeNotes(clip, ids);
  };

  const selectCreated = (created: Note[] | undefined) => {
    if (created) deps.setSelected(new Set<number>(created.map((n) => n.id)));
  };

  const copySelected = () => deps.notes.copy(deps.selectedNotes());

  const paste = async () => {
    const clip = deps.target();
    if (clip !== null) selectCreated(await deps.notes.paste(clip, deps.pasteAt()));
  };

  const duplicate = async () => {
    const clip = deps.target();
    const group = deps.selectedNotes();
    const copy = copyNotes(group);
    if (clip === null || !copy) return;
    const notes: NewNote[] = pasteNotes(copy, duplicateAt(group, deps.quantizeStep()));
    selectCreated(await deps.notes.addNotes(clip, notes));
  };

  /** Quantizes the selected notes to the current grid, or all without a selection (the rule is in the backend, `notes.quantize`). */
  const quantize = async () => {
    const clip = deps.target();
    const ids = [...deps.selected()];
    if (clip !== null && deps.noteList().length > 0) await deps.notes.quantize(clip, ids.length ? ids : undefined, deps.quantizeStep());
  };

  const nudge = (deltaTick: number, deltaPitch: number) => applyChanges(moveNotes(deps.selectedNotes(), deltaTick, deltaPitch));

  /** Arrows move the selection: ↑↓ — pitch (Shift — octave), ←→ — grid step (Shift — bar). */
  const nudgeByArrow = (key: string, big: boolean) => {
    const pitch = big ? OCTAVE : SEMITONE;
    const tick = big ? deps.barTicks() : deps.gridStep();
    if (key === "ArrowUp") return nudge(0, pitch);
    if (key === "ArrowDown") return nudge(0, -pitch);
    if (key === "ArrowLeft") return nudge(-tick, 0);
    if (key === "ArrowRight") return nudge(tick, 0);
  };

  /** Panel keys; handled ones are consumed (`preventDefault`) so the window-wide keys (timeline clips) leave them alone. */
  const onKeyDown = (event: KeyboardEvent) => {
    if ((event.target as HTMLElement).closest("input, select, textarea")) return;
    const key = NOTE_KEYS.find((k) => k.matches(event, event.metaKey || event.ctrlKey));
    const hasSelection = deps.selected().size > 0;
    if (!key || ((key.action === "deselect" || key.action === "nudge") && !hasSelection)) return;
    const runners: Record<NoteKeyAction, () => unknown> = {
      deselect: () => deps.setSelected(new Set<number>()),
      remove: removeSelected,
      selectAll: () => deps.setSelected(new Set(deps.noteList().map((n) => n.id))),
      copy: copySelected,
      cut: () => { copySelected(); return removeSelected(); },
      paste,
      duplicate,
      quantize,
      nudge: () => nudgeByArrow(event.key, event.shiftKey),
    };
    event.preventDefault();
    void runners[key.action]();
  };

  return { applyChanges, removeSelected, copySelected, paste, duplicate, quantize, onKeyDown };
}
