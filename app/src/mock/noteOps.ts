import type { Clip, ControllerKind, ControllerLane, ControllerPoint, NewNote, Note, NoteContent } from "../api/types";
import { controllerRange, sameKind } from "../pianoroll/controllers";
import { BAR_TICKS, MAX_CLIP_LOOP_TICKS, STEP_TICKS } from "../api/types";
import { type ErrorCode, fail } from "./errors";

export const MAX_NOTES_PER_CLIP = 2048;

export function contentOf(clip: Clip): NoteContent {
  if (clip.content.type !== "notes") throw fail("NT-110", { id: clip.id });
  return clip.content;
}

/** Auto length follows the notes: up to the end of the last occupied bar of length `bar` (the bar at the clip start), at least one bar. */
export function fitLength(content: NoteContent, bar: number) {
  if (!content.autoLength) return;
  const lastPoint = Math.max(0, ...(content.controllers ?? []).map((lane) => (lane.points.at(-1)?.tick ?? -1) + 1));
  const lastEnd = Math.max(lastPoint, ...content.notes.map((n) => n.startTick + n.lengthTicks));
  content.loopTicks = Math.min(MAX_CLIP_LOOP_TICKS, Math.max(1, Math.ceil(lastEnd / bar)) * bar);
}

/** How far a note may extend the content. */
export const noteLimit = (content: NoteContent) => (content.autoLength ? MAX_CLIP_LOOP_TICKS : content.loopTicks);

export function validateNote(note: NewNote, limit: number) {
  if (!(note.pitch >= 0 && note.pitch <= 127)) throw fail("NT-201", { pitch: note.pitch });
  if (!(note.velocity >= 1 && note.velocity <= 127)) throw fail("NT-202", { velocity: note.velocity });
  const end = note.startTick + note.lengthTicks;
  if (note.lengthTicks < 1 || end > limit) throw fail("NT-203", { end, max: limit });
}

/** Empty content (`NoteContent::empty`); `fitLength` sets the one-bar length of the clip. */
export const emptyNotes = (): Extract<Clip["content"], { type: "notes" }> => ({
  type: "notes", notes: [], nextNoteId: 1, loopTicks: BAR_TICKS, autoLength: true,
});

/** At most `max` items; `code` is the backend's error for this limit. */
export function limitCount(count: number, max = MAX_NOTES_PER_CLIP, code: ErrorCode = "NT-210") {
  if (count > max) throw fail(code, { max });
}

export const createNote = (content: NoteContent, note: NewNote): Note => ({ id: content.nextNoteId++, ...note });

/** Replaces all notes of the content; returns their count. */
export function replaceNotes(content: NoteContent, notes: NewNote[]): number {
  limitCount(notes.length);
  const limit = noteLimit(content);
  content.notes = notes.map((n) => { validateNote(n, limit); return createNote(content, n); });
  return content.notes.length;
}

/** Manual content length: a multiple of the step, from one step to `MAX_CLIP_LOOP_TICKS`. */
export function checkLoopTicks(loopTicks: number) {
  if (!(loopTicks >= STEP_TICKS && loopTicks <= MAX_CLIP_LOOP_TICKS) || loopTicks % STEP_TICKS !== 0) {
    throw fail("NT-211", { ticks: loopTicks, step: STEP_TICKS, max: MAX_CLIP_LOOP_TICKS });
  }
}

/** Sets a checked manual length: auto length turns off, notes past it are dropped, long ones are trimmed. */
export function setLoopTicks(content: NoteContent, loopTicks: number) {
  content.autoLength = false;
  content.loopTicks = loopTicks;
  content.notes = content.notes
    .filter((n) => n.startTick < loopTicks)
    .map((n) => ({ ...n, lengthTicks: Math.min(n.lengthTicks, loopTicks - n.startTick) }));
  setLanes(content, (content.controllers ?? []).map((lane) => ({ ...lane, points: lane.points.filter((p) => p.tick < loopTicks) })));
}

/** Drops empty lanes; no lanes means no field (as the backend's `skip_serializing_if`). */
function setLanes(content: NoteContent, lanes: ControllerLane[]) {
  const kept = lanes.filter((lane) => lane.points.length > 0);
  if (kept.length > 0) content.controllers = kept;
  else delete content.controllers;
}

export const MAX_CONTROLLER_POINTS = 4096;
const MAX_CC_NUMBER = 119;

/**
 * Replaces the points of controller lane `kind` (empty removes the lane), as `note_rules::set_controller_lane`: CC number,
 * point limit, ticks within the content, values within the kind's range; sorted by tick, the last of equal ticks wins.
 */
export function setControllerLane(content: NoteContent, kind: ControllerKind, points: ControllerPoint[]) {
  if (kind.type === "cc" && !(kind.number >= 0 && kind.number <= MAX_CC_NUMBER)) {
    throw fail("NT-204", { number: kind.number, max: MAX_CC_NUMBER });
  }
  limitCount(points.length, MAX_CONTROLLER_POINTS, "NT-212");
  const limit = noteLimit(content);
  const [min, max] = controllerRange(kind);
  for (const p of points) {
    if (!(p.tick >= 0 && p.tick < limit)) throw fail("NT-213", { tick: p.tick, max: limit - 1 });
    if (!(Number.isFinite(p.value) && p.value >= min && p.value <= max)) {
      throw fail("NT-114", { value: p.value, min, max });
    }
  }
  const byTick = new Map(points.map((p) => [p.tick, { tick: p.tick, value: p.value }]));
  const sorted = [...byTick.values()].sort((a, b) => a.tick - b.tick);
  const others = (content.controllers ?? []).filter((lane) => !sameKind(lane.kind, kind));
  setLanes(content, [...others, { kind, points: sorted }]);
}
