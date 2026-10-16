// Audio clip takes as seen by the UI: the active one is `content.sample`, the list is `content.takes`
// (a clip without takes has just its own sound), the comp of pieces is `content.comp`. The rules for editing takes and comps
// live in the backend and the mock (`mock/takeRules.ts`).
import type { ClipContent, CompSegment, Id, TempoMap } from "../api/types";
import { snapNearestInBars } from "./tempoMap";

/** All clip takes in recording order (a clip without takes has just its own sound). */
export function takesOf(content: ClipContent): Id[] {
  if (content.type !== "audio") return [];
  return content.takes && content.takes.length > 0 ? content.takes : [content.sample];
}

/** The clip's comp (empty means the whole clip plays the active take). */
export function compOf(content: ClipContent): CompSegment[] {
  return content.type === "audio" ? (content.comp ?? []) : [];
}

/** Clip piece `[from, to)` (ticks from the clip start) played by take `take`. */
export interface TakeRange {
  take: Id;
  from: number;
  to: number;
}

/** What plays on each piece of a clip of length `lengthTicks`: the comp segments, or without a comp the whole active take. */
export function takeRanges(content: ClipContent, lengthTicks: number): TakeRange[] {
  if (content.type !== "audio") return [];
  const comp = compOf(content);
  if (comp.length === 0) return [{ take: content.sample, from: 0, to: lengthTicks }];
  return comp.map((segment, i) => ({ take: segment.take, from: segment.startTicks, to: Math.min(comp[i + 1]?.startTicks ?? lengthTicks, lengthTicks) }));
}

/** The take that plays at tick `tick` from the clip start. */
export function takeAt(content: ClipContent, tick: number): Id | null {
  if (content.type !== "audio") return null;
  const comp = compOf(content);
  let take = content.sample;
  for (const segment of comp) if (segment.startTicks <= tick) take = segment.take;
  return take;
}

/**
 * Clip piece chosen by dragging along a take lane from tick `a` to `b` (from the clip start, any order):
 * ends snap to grid `grid` by map bars (as on the timeline) and are clipped to the clip. `null` if the piece is empty.
 */
export function draggedRange(
  map: TempoMap, clip: { startTick: number; lengthTicks: number }, a: number, b: number, grid: number,
): { from: number; to: number } | null {
  const snap = (tick: number) => {
    const local = snapNearestInBars(map, clip.startTick + tick, grid) - clip.startTick;
    return Math.min(clip.lengthTicks, Math.max(0, Math.round(local)));
  };
  const [from, to] = [snap(Math.min(a, b)), snap(Math.max(a, b))];
  return to > from ? { from, to } : null;
}
