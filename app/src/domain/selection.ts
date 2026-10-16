// Selected notes: marquee, group move, clipboard, duplication and what a mouse gesture changed.
import type { Id, NewNote, Note } from "../api/types";
import { clamp } from "./math";
import { HIGH_PITCH, LOW_PITCH, clampPitch } from "./pitch";
import { stripId } from "./pianoRoll";

/** Rectangle in ticks and pitches (note marquee); bounds may be given in any order. */
export interface TickPitchRect {
  fromTick: number;
  toTick: number;
  fromPitch: number;
  toPitch: number;
}

/** Edit of one note for `notes.updateMany`. */
export interface NoteChange {
  id: Id;
  note: NewNote;
}

/** Notes that intersect the selection rectangle. */
export function notesInRect(notes: Note[], rect: TickPitchRect): Note[] {
  const [t0, t1] = [Math.min(rect.fromTick, rect.toTick), Math.max(rect.fromTick, rect.toTick)];
  const [p0, p1] = [Math.min(rect.fromPitch, rect.toPitch), Math.max(rect.fromPitch, rect.toPitch)];
  return notes.filter((n) => n.pitch >= p0 && n.pitch <= p1 && n.startTick < t1 && n.startTick + n.lengthTicks > t0);
}

/** Moves notes in time and pitch; start never goes below zero, pitch stays in the editor range. */
export function moveNotes(notes: Note[], deltaTick: number, deltaPitch: number): NoteChange[] {
  if (notes.length === 0) return [];
  // The group moves as a whole: limits come from the outermost notes so the shape is not distorted.
  const minStart = Math.min(...notes.map((n) => n.startTick));
  const minPitch = Math.min(...notes.map((n) => n.pitch));
  const maxPitch = Math.max(...notes.map((n) => n.pitch));
  const tick = Math.max(deltaTick, -minStart);
  const pitch = clamp(deltaPitch, LOW_PITCH - minPitch, HIGH_PITCH - maxPitch);
  return notes.map((n) => ({ id: n.id, note: { ...stripId(n), startTick: n.startTick + tick, pitch: n.pitch + pitch } }));
}

/** Gesture drafts that actually differ from the original notes (unchanged ones are not sent). */
export function changedNotes(drafts: Record<Id, NewNote>, originals: Note[]): NoteChange[] {
  const byId = new Map(originals.map((n) => [n.id, n]));
  return Object.entries(drafts)
    .map(([id, note]) => ({ id: Number(id), note }))
    .filter(({ id, note }) => {
      const original = byId.get(id);
      return !original || (Object.keys(note) as (keyof NewNote)[]).some((key) => note[key] !== original[key]);
    });
}

/** Copied notes: start and pitch are relative to the earliest note of the group. */
export interface NoteClipboard {
  notes: (Omit<NewNote, "startTick" | "pitch"> & { offsetTick: number; pitch: number })[];
  span: number;
}

export function copyNotes(notes: Note[]): NoteClipboard | null {
  if (notes.length === 0) return null;
  const origin = Math.min(...notes.map((n) => n.startTick));
  const end = Math.max(...notes.map((n) => n.startTick + n.lengthTicks));
  return {
    notes: notes.map((n) => ({ offsetTick: n.startTick - origin, pitch: n.pitch, lengthTicks: n.lengthTicks, velocity: n.velocity })),
    span: end - origin,
  };
}

/** Clipboard notes placed so that the group starts at `atTick`. */
export function pasteNotes(clipboard: NoteClipboard, atTick: number, deltaPitch = 0): NewNote[] {
  const start = Math.max(0, atTick);
  return clipboard.notes.map((n) => ({
    pitch: clampPitch(n.pitch + deltaPitch),
    startTick: start + n.offsetTick,
    lengthTicks: n.lengthTicks,
    velocity: n.velocity,
  }));
}

/** Where to put the copy when duplicating: right after the group, snapped to the grid. */
export function duplicateAt(notes: Note[], grid: number): number {
  const origin = Math.min(...notes.map((n) => n.startTick));
  const end = Math.max(...notes.map((n) => n.startTick + n.lengthTicks));
  const span = Math.max(grid, Math.ceil((end - origin) / grid) * grid);
  return origin + span;
}

export function toggleInSet(selection: Set<number>, id: number): Set<number> {
  const next = new Set(selection);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}
