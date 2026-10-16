// Mock rules for audio clip takes and comping, as in the backend (`daw-project/src/take_rules.rs`): the clip's place
// and length do not change, the active take is `content.sample`, the comp is `content.comp` (ascending, the first at 0,
// neighbours are different takes, a single take means no comp). A refusal is a thrown backend error (`fail`).
import type { ClipContent, CompSegment, Id } from "../api/types";
import { compOf, takeAt, takesOf } from "../domain/takes";
import { fail } from "./errors";

type AudioContent = Extract<ClipContent, { type: "audio" }>;

/** The backend's `MAX_COMP_SEGMENTS`. */
const MAX_COMP_SEGMENTS = 256;

function audio(content: ClipContent): AudioContent {
  if (content.type !== "audio") throw fail("CL-121");
  return content;
}

/** Content with a canonical comp: neighbours of the same take merge; a single one left means no comp, and it becomes active. */
function withComp(content: AudioContent, segments: CompSegment[]): AudioContent {
  const merged: CompSegment[] = [];
  for (const segment of segments) if (merged[merged.length - 1]?.take !== segment.take) merged.push(segment);
  const { comp: _old, ...rest } = content;
  if (merged.length === 1) return { ...rest, sample: merged[0]!.take };
  return merged.length > 1 ? { ...rest, comp: merged } : rest;
}

/** A new active take for the whole clip (the comp is dropped); the previous sound becomes the first in the list if there was no list yet. */
export function addTake(content: ClipContent, take: Id): AudioContent {
  const current = audio(content);
  const takes = takesOf(current);
  if (takes.includes(take)) throw fail("CL-122", { take });
  return { type: "audio", sample: take, takes: [...takes, take] };
}

/** A whole take: the comp is dropped. */
export function activateTake(content: ClipContent, take: Id): AudioContent {
  const current = audio(content);
  if (!takesOf(current).includes(take)) throw fail("CL-127", { take });
  return withComp({ ...current, sample: take }, []);
}

/** `take` is the clip's only sound: removing it means removing the clip. */
export function isOnlySound(content: ClipContent, take: Id): boolean {
  return content.type === "audio" && takesOf(content).every((t) => t === take) && content.sample === take;
}

/** Removes a take: if it was active, the clip switches to the last remaining one; its comp segments go to neighbours; with one left the list empties. */
export function removeTake(content: ClipContent, take: Id): AudioContent {
  if (isOnlySound(content, take)) throw fail("CL-128", { take });
  const current = audio(content);
  if (!(current.takes ?? []).includes(take)) throw fail("CL-127", { take });
  const takes = (current.takes ?? []).filter((t) => t !== take);
  const sample = current.sample === take ? takes[takes.length - 1]! : current.sample;
  const left = compOf(current).filter((s) => s.take !== take).map((s, i) => (i === 0 ? { ...s, startTicks: 0 } : s));
  const next = withComp({ ...current, sample, takes }, left);
  if (takes.length > 1) return next;
  return { type: "audio", sample: next.sample };
}

/** The whole comp for a clip of length `length` (rules: `TakeControl::set_comp`). */
export function setComp(content: ClipContent, length: number, segments: CompSegment[]): AudioContent {
  const current = audio(content);
  if (segments.length > MAX_COMP_SEGMENTS) throw fail("CL-215", { max: MAX_COMP_SEGMENTS });
  if (segments.length > 0 && segments[0]!.startTicks !== 0) throw fail("CL-123", { tick: segments[0]!.startTicks });
  segments.forEach((s, i) => {
    const before = segments[i - 1]?.startTicks;
    if (before !== undefined && s.startTicks <= before) throw fail("CL-124", { before, tick: s.startTicks });
    if (s.startTicks >= length) throw fail("CL-125", { tick: s.startTicks, length });
    if (!takesOf(current).includes(s.take)) throw fail("CL-127", { take: s.take });
  });
  return withComp(current, segments.map((s) => ({ startTicks: s.startTicks, take: s.take })));
}

/** The `[from, to)` range of a clip of length `length` plays take `take`; the rest stays as it was. */
export function chooseTakeRange(content: ClipContent, length: number, from: number, to: number, take: Id): AudioContent {
  const [start, end] = [Math.min(from, length), Math.min(to, length)];
  if (start >= end) throw fail("CL-126", { from, to });
  const current = audio(content);
  const segments = compOf(current).length > 0 ? compOf(current) : [{ startTicks: 0, take: current.sample }];
  const next = segments.filter((s) => s.startTicks < start);
  next.push({ startTicks: start, take });
  if (end < length) next.push({ startTicks: end, take: takeAt(current, end) ?? take });
  next.push(...segments.filter((s) => s.startTicks > end));
  return setComp(current, length, next);
}

/** The comp after a clip length change: segments past the new end are dropped. */
export function fitComp(content: ClipContent, length: number): ClipContent {
  if (content.type !== "audio" || compOf(content).length === 0) return content;
  return withComp(content, compOf(content).filter((s) => s.startTicks < length));
}

/** A split at tick `at` from the clip start: content of the left and right parts. */
export function splitComp(content: ClipContent, at: number): [ClipContent, ClipContent] {
  return [fitComp(content, at), shiftComp(content, at)];
}

/**
 * The clip start moved by `shift` ticks with the sound staying in place (`clips.trimStart`): a positive shift drops the
 * comp's beginning, a negative one extends the first segment. The caller fits the comp to the new length.
 */
export function shiftComp(content: ClipContent, shift: number): ClipContent {
  if (content.type !== "audio" || compOf(content).length === 0) return content;
  const comp = compOf(content);
  if (shift < 0) return withComp(content, comp.map((s, i) => ({ ...s, startTicks: i === 0 ? 0 : s.startTicks - shift })));
  return withComp(content, [
    { startTicks: 0, take: takeAt(content, shift)! },
    ...comp.filter((s) => s.startTicks > shift).map((s) => ({ ...s, startTicks: s.startTicks - shift })),
  ]);
}

/**
 * The track's audio clip that recording `[start, end)` covered at least by half or that contains it entirely;
 * of several, the one covered most.
 */
export function coveredClip<C extends { track: Id; startTick: number; lengthTicks: number; content: ClipContent }>(
  clips: C[], track: Id, start: number, end: number,
): C | null {
  let best: C | null = null;
  let bestCovered = 0;
  for (const clip of clips) {
    if (clip.track !== track || clip.content.type !== "audio") continue;
    const covered = Math.min(end, clip.startTick + clip.lengthTicks) - Math.max(start, clip.startTick);
    if (covered > 0 && (covered * 2 >= clip.lengthTicks || covered >= end - start) && covered > bestCovered) {
      best = clip;
      bestCovered = covered;
    }
  }
  return best;
}
