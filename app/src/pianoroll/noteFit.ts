import { STEP_TICKS, type NewNote, type Note } from "../api/types";
import { hitSpan, type HitZone } from "../tracks/clipRules";
import { clamp } from "../common/math";
import { clampPitch } from "../common/pitch";

/** Fits a note into the clip content: start within bounds, length at least one step. */
export function fitNote(note: NewNote, contentTicks: number, grid = STEP_TICKS): NewNote {
  const lengthTicks = clamp(note.lengthTicks, grid, contentTicks);
  const startTick = clamp(note.startTick, 0, contentTicks - lengthTicks);
  return { ...note, pitch: clampPitch(note.pitch), startTick, lengthTicks };
}

export interface NoteHit {
  note: Note;
  zone: HitZone;
}

export function hitTest(notes: Note[], tick: number, pitch: number, handleTicks: number): NoteHit | null {
  for (let i = notes.length - 1; i >= 0; i--) {
    const note = notes[i]!;
    const zone = note.pitch === pitch ? hitSpan(note.startTick, note.lengthTicks, tick, handleTicks) : null;
    if (zone) return { note, zone };
  }
  return null;
}

/** A note without its id: what is sent to `notes.updateMany`. */
export const stripId = ({ id: _id, ...rest }: Note): NewNote => rest;
