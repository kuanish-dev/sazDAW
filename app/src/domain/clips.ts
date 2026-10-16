// Clips on the timeline: bounds, mouse hits, placement on a track (overlaps and crossfades), edits a drag makes (trim the
// start, resize, gain), content repeats, label, song end.
import { PPQ, type Clip, type Id, type Project, type TempoMap } from "../api/types";
import { t } from "../i18n";
import type { Placement } from "./clipSelection";
import { clamp } from "./math";
import { volumeToDb } from "./mixer";
import { compOf, takesOf } from "./takes";
import { barLengthAt, snapNearestInBars } from "./tempoMap";
import { BAR_GRID, FREE_GRID } from "./time";
import { sourceFramesToTicks, ticksToSourceFrames, warpMarks } from "./warp";

/** Maximum clip gain (linear, ≈ +12 dB), as the backend's `MAX_CLIP_GAIN`. */
export const MAX_CLIP_GAIN = 4;
/** Gain drag speed: dB per pixel (with Shift ten times finer); below `SILENT_DB` the gain becomes 0. */
const GAIN_DB_PER_PX = 0.25;
const SILENT_DB = -60;

/** `resize` — the right edge (length), `trim` — the left edge (start and offset together). */
export type HitZone = "body" | "resize" | "trim";

export interface ClipHit {
  clip: Clip;
  zone: HitZone;
}

/**
 * Hit of point `at` in span [start, start + length): `resize` — the right handle of width `handle`
 * (at most half the span), `body` — the rest, `null` — a miss. Shared by clips and notes.
 */
export function hitSpan(start: number, length: number, at: number, handle: number): HitZone | null {
  const end = start + length;
  if (at < start || at >= end) return null;
  return at >= end - Math.min(handle, length / 2) ? "resize" : "body";
}

export function clipEnd(clip: Clip): number {
  return clip.startTick + clip.lengthTicks;
}

/** The clip under the point; the left handle (at most a quarter of the clip) trims its start. */
export function hitClip(clips: Clip[], trackId: Id, tick: number, handleTicks: number): ClipHit | null {
  for (let i = clips.length - 1; i >= 0; i--) {
    const clip = clips[i]!;
    const zone = clip.track === trackId ? hitSpan(clip.startTick, clip.lengthTicks, tick, handleTicks) : null;
    if (zone === "body" && tick < clip.startTick + Math.min(handleTicks, clip.lengthTicks / 4)) return { clip, zone: "trim" };
    if (zone) return { clip, zone };
  }
  return null;
}

/**
 * The clip with its left edge at `start` (before its end) and the content in place, as `clips.trimStart` does: a note
 * clip's loop phase turns, an audio clip's offset moves by the frames those ticks play (`sampleRate` of its sound) and
 * stops at the file start. The comp is not shifted (the backend does it); this is for drawing and the mock.
 */
export function trimmedClip(clip: Clip, start: number, map: TempoMap, sampleRate: number): Clip {
  const end = clipEnd(clip);
  let from = start;
  const trimmed: Clip = { ...clip };
  if (clip.content.type === "notes") {
    const loop = Math.max(1, clip.content.loopTicks);
    trimmed.offsetTicks = (((clip.offsetTicks + from - clip.startTick) % loop) + loop) % loop;
  } else {
    const framesFrom = (tick: number) => clip.offsetFrames + ticksToSourceFrames(clip, tick - clip.startTick, map, sampleRate);
    // Nothing before the file start: the edge stops where the offset reaches zero.
    if (framesFrom(from) < 0) from = Math.max(0, Math.ceil(clip.startTick + sourceFramesToTicks(clip, -clip.offsetFrames, map, sampleRate)));
    trimmed.offsetFrames = Math.round(Math.max(0, framesFrom(from)));
  }
  const length = end - from;
  return { ...trimmed, startTick: from, lengthTicks: length, fadeInTicks: Math.min(clip.fadeInTicks, length), fadeOutTicks: Math.min(clip.fadeOutTicks, length) };
}

/** Whether [start, start+length) overlaps other clips of the track (`ignore` — the clip itself). */
export function overlapsOthers(clips: Clip[], trackId: Id, start: number, length: number, ignore?: Id): boolean {
  return clips.some((c) => c.track === trackId && c.id !== ignore && c.startTick < start + length && start < clipEnd(c));
}

/**
 * What's wrong with placement on the track (as in the backend, `clip_rules.rs`): note clips don't overlap; audio clips may
 * overlap at the edges (crossfade), but no more than two at a point and none inside another. `null` — all good.
 */
export function trackPlacementProblem(clips: Clip[], trackId: Id, audio: boolean): string | null {
  const spans = clips.filter((c) => c.track === trackId).map((c) => [c.startTick, clipEnd(c)] as const)
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  for (let i = 0; i + 1 < spans.length; i++) {
    const [start, end] = spans[i]!;
    const [nextStart, nextEnd] = spans[i + 1]!;
    if (nextStart >= end) continue;
    if (!audio) return t().tracks.placement.notesOverlap;
    if (nextStart === start || nextEnd <= end) return t().tracks.placement.audioInside;
    if (i + 2 < spans.length && spans[i + 2]![0] < end) return t().tracks.placement.tooManyAudio;
  }
  return null;
}

/** Whether clip [start, start+length) fits on the track (`audio` — audio track: edge overlaps allowed). */
export function canPlace(clips: Clip[], trackId: Id, start: number, length: number, audio: boolean): boolean {
  const probe = { id: -1, track: trackId, startTick: start, lengthTicks: length } as Clip;
  return trackPlacementProblem([...clips, probe], trackId, audio) === null;
}

/** A crossfade is the overlap of two audio clips on one track. */
export interface Crossfade {
  track: Id;
  start: number;
  end: number;
}

/** All crossfades: pairs of overlapping audio clips (placement rules allow at most two at a point). */
export function crossfades(clips: Clip[]): Crossfade[] {
  const audio = clips.filter((c) => c.content.type === "audio").sort((a, b) => a.track - b.track || a.startTick - b.startTick);
  const result: Crossfade[] = [];
  for (let i = 0; i < audio.length; i++) {
    for (let j = i + 1; j < audio.length && audio[j]!.track === audio[i]!.track && audio[j]!.startTick < clipEnd(audio[i]!); j++) {
      result.push({ track: audio[i]!.track, start: audio[j]!.startTick, end: Math.min(clipEnd(audio[i]!), clipEnd(audio[j]!)) });
    }
  }
  return result;
}

/** Shortest clip at `start` when resizing: a 64th note without snapping; with "Bar", the bar there; else the grid step. */
export const minClipLength = (start: number, snap: number, map: TempoMap): number =>
  snap === FREE_GRID ? PPQ / 16 : snap === BAR_GRID ? barLengthAt(map, start) : snap;

/** Trimming the left edge: the start snaps to the grid and stays at least the minimum length before the end. */
export function trimStartAt(clip: Clip, tick: number, snap: number, map: TempoMap): number {
  const start = snapNearestInBars(map, tick, snap);
  return clamp(start, 0, clipEnd(clip) - minClipLength(clip.startTick, snap, map));
}

/** Resizing the right edge: the end snaps to the grid by the map's bars. */
export function resizePlacement(clip: Clip, tick: number, snap: number, map: TempoMap): Placement {
  const end = snapNearestInBars(map, tick, snap);
  return { track: clip.track, startTick: clip.startTick, lengthTicks: Math.max(minClipLength(clip.startTick, snap, map), end - clip.startTick) };
}

/** Gain after dragging the gain line `dy` pixels down from where it was `gain` (up makes it louder). */
export function draggedGain(gain: number, dy: number, fine: boolean): number {
  const db = Math.max(SILENT_DB, volumeToDb(gain)) - dy * GAIN_DB_PER_PX * (fine ? 0.1 : 1);
  return db <= SILENT_DB ? 0 : Math.min(MAX_CLIP_GAIN, 10 ** (db / 20));
}

/** Where looped content starts again inside a clip `start…end` (note or automation clip): song ticks, its start excluded. */
export function repeatTicks(start: number, end: number, loop: number): number[] {
  const ticks: number[] = [];
  if (loop <= 0) return ticks;
  for (let at = start + loop; at < end; at += loop) ticks.push(at);
  return ticks;
}

/** Song end is the end of the last clip. */
export function songEnd(clips: Clip[]): number {
  return clips.reduce((end, c) => Math.max(end, clipEnd(c)), 0);
}

/**
 * Clip label on the timeline: sample name for audio (with a take mark "⧉ 2/3" or comp "⧉ comp", warp "⟲ 128"
 * and pitch), track name for notes.
 */
export function clipLabel(clip: Clip, project: Pick<Project, "samples" | "tracks">): string {
  const content = clip.content;
  if (content.type === "notes") return project.tracks.find((track) => track.id === clip.track)?.name ?? t().tracks.notesFallback;
  const name = project.samples.find((s) => s.id === content.sample)?.name ?? t().tracks.audioFallback;
  const takes = takesOf(content);
  const take = compOf(content).length > 0 ? "⧉ comp" : takes.length > 1 ? `⧉ ${takes.indexOf(content.sample) + 1}/${takes.length}` : "";
  const marks = [take, warpMarks(clip)].filter(Boolean).join(" ");
  return marks ? `${name}  ${marks}` : name;
}
