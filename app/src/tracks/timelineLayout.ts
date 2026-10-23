import { BAR_TICKS, type Clip } from "../api/types";
import type { GroupPlacement } from "./clipSelection";
import { songEnd } from "./clipRules";
import { type RowLayout, rowHeight, rowTop } from "./rowLayout";
import type { FadeEdge } from "./fades";
import { volumeToDb } from "../mixer/mixer";
import { clamp } from "../common/math";

/** Bar ruler (numbers, signatures, markers, loop, cue): a canvas of its own that stays on screen above the rows
 *  (`Timeline`, `TracksView`). Row coordinates start at the top of the rows, below the ruler. */
export const RULER_HEIGHT = 28;
export const HANDLE_PX = 8;
export const MIN_BARS = 16;
/** How many empty bars to show after the last clip. */
const TAIL_BARS = 4;
export const ZOOM_MIN = 0.1;
export const ZOOM_MAX = 8;
export const BASE_PX_PER_TICK = 0.05;
/** Wheel zoom sensitivity: ~10% per mouse wheel notch; trackpad bursts are capped by `WHEEL_DELTA_LIMIT`. */
const WHEEL_ZOOM_RATE = 0.001;
const WHEEL_DELTA_LIMIT = 100;
const CLIP_PADDING = 4;
const MIN_CLIP_PX = 4;

/** Space at the top for the clip name, and at the bottom. */
const PREVIEW_TOP = 18;
const PREVIEW_VERTICAL_PAD = 20;
/** Minimum pitch span in semitones (an octave), so a single note doesn't stretch over the whole clip. */
const PREVIEW_MIN_SPAN = 11;
const PREVIEW_NOTE_MIN_H = 2;
const PREVIEW_NOTE_MAX_H = 5;

/** Rectangle in canvas pixels. */
export interface PixelRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Drag draft: new clip positions; `copy` means Alt-drag, the originals stay in place. `edited` is one clip changed
 * as a whole (trimmed start, fade, gain).
 */
export interface ClipDraft {
  placements: GroupPlacement[];
  copy: boolean;
  edited?: Clip;
}

export type Tool = "draw" | "split" | "erase";

/** New zoom after a wheel step: up zooms in, down zooms out (smoothly). */
export function zoomByWheel(zoom: number, deltaY: number): number {
  const next = zoom * Math.exp(-clamp(deltaY, -WHEEL_DELTA_LIMIT, WHEEL_DELTA_LIMIT) * WHEEL_ZOOM_RATE);
  return clamp(next, ZOOM_MIN, ZOOM_MAX);
}

/** Scroll offset that keeps the tick under the cursor in place after a zoom change. */
export function anchoredScroll(tick: number, cursorX: number, pxPerTick: number, canvasLeftInContent: number): number {
  return Math.max(0, canvasLeftInContent + tick * pxPerTick - cursorX);
}

/**
 * How many bars to show: at least `MIN_BARS`, with room after the last clip (`automationEnd` — the end of the last
 * automation clip). This only reserves space
 * (the strip width), so a bar here is a fixed 4/4; the song's bars are drawn from the tempo map (`gridLines`).
 */
export function visibleBars(clips: Clip[], automationEnd = 0): number {
  return Math.max(MIN_BARS, Math.ceil(Math.max(songEnd(clips), automationEnd) / BAR_TICKS) + TAIL_BARS);
}

/** Tick under canvas x (never before the song start). */
export const tickAtX = (x: number, pxPerTick: number): number => Math.max(0, x / pxPerTick);

/**
 * Where a span of ticks sits on row `row`: inset from the row edges, at least a few pixels wide. Every clip-shaped thing
 * on the rows uses it — note, audio and automation clips, crossfades, the drop ghost — so they line up.
 */
export function spanRect(startTick: number, lengthTicks: number, row: number, pxPerTick: number, layout: RowLayout): PixelRect {
  return {
    x: startTick * pxPerTick,
    y: rowTop(row, layout) + CLIP_PADDING,
    w: Math.max(MIN_CLIP_PX, lengthTicks * pxPerTick - 1),
    h: rowHeight(row, layout) - CLIP_PADDING * 2,
  };
}

export const clipRect = (clip: Clip, row: number, pxPerTick: number, layout: RowLayout): PixelRect =>
  spanRect(clip.startTick, clip.lengthTicks, row, pxPerTick, layout);

/** Mini notes inside a clip (canvas coordinates); content repeats every `loopTicks`, shifted by `offsetTicks`. */
export function notePreview(clip: Clip, rect: PixelRect, pxPerTick: number): PixelRect[] {
  if (clip.content.type !== "notes" || clip.content.notes.length === 0) return [];
  const { notes, loopTicks } = clip.content;
  if (loopTicks <= 0) return [];
  const low = Math.min(...notes.map((n) => n.pitch));
  const high = Math.max(...notes.map((n) => n.pitch));
  const span = Math.max(PREVIEW_MIN_SPAN, high - low + 1);
  const area = rect.h - PREVIEW_VERTICAL_PAD;
  const noteH = clamp(area / span, PREVIEW_NOTE_MIN_H, PREVIEW_NOTE_MAX_H);
  const result: PixelRect[] = [];
  for (let base = -clip.offsetTicks; base < clip.lengthTicks; base += loopTicks) {
    for (const note of notes) {
      const start = base + note.startTick;
      const end = Math.min(clip.lengthTicks, start + note.lengthTicks);
      if (end <= 0 || start >= clip.lengthTicks) continue;
      const from = Math.max(0, start);
      result.push({
        x: rect.x + from * pxPerTick,
        y: rect.y + PREVIEW_TOP + ((high - note.pitch) / span) * area,
        w: Math.max(2, (end - from) * pxPerTick - 1),
        h: noteH,
      });
    }
  }
  return result;
}

/** Clips to draw with the draft applied: moved clips or (for a copy) ghosts next to the originals. */
export function applyDraft(clips: Clip[], draft: ClipDraft | null): Clip[] {
  if (!draft) return clips;
  const edited = draft.edited;
  if (edited) return clips.map((c) => (c.id === edited.id ? edited : c));
  const moved = new Map(draft.placements.map((p) => [p.id, p]));
  if (!draft.copy) return clips.map((c) => (moved.has(c.id) ? { ...c, ...moved.get(c.id)! } : c));
  const ghosts = clips.filter((c) => moved.has(c.id)).map((c) => ({ ...c, ...moved.get(c.id)!, id: -c.id }));
  return [...clips, ...ghosts];
}

/** Half the size of a fade handle square, and how far from the gain line a press still grabs it (pixels). */
export const FADE_HANDLE_PX = 4;
const GAIN_GRAB_PX = 3;
/** The gain line spans this dB range over the clip height (0 dB sits at three quarters). */
const GAIN_LINE_MIN_DB = -36;
const GAIN_LINE_MAX_DB = 12;

/** What a press grabs on an audio clip: a fade handle or the gain line. */
export type AudioZone = { kind: "fade"; edge: FadeEdge } | { kind: "gain" };

/** X of a fade handle: the fade's end on the clip's top edge. */
export function fadeHandleX(clip: Clip, rect: PixelRect, edge: FadeEdge, pxPerTick: number): number {
  return edge === "in" ? rect.x + clip.fadeInTicks * pxPerTick : rect.x + rect.w - clip.fadeOutTicks * pxPerTick;
}

/** Y of the gain line in the clip. */
export const showsGainLine = (clip: Clip, selected: boolean) => selected || clip.gain !== 1;

export function gainLineY(rect: PixelRect, gain: number): number {
  const db = clamp(volumeToDb(gain), GAIN_LINE_MIN_DB, GAIN_LINE_MAX_DB);
  return rect.y + rect.h - ((db - GAIN_LINE_MIN_DB) / (GAIN_LINE_MAX_DB - GAIN_LINE_MIN_DB)) * rect.h;
}

/** `gainLine`: the gain line is drawn (and so can be grabbed) — the clip is selected or its gain is not 0 dB. */
export function audioZoneAt(clip: Clip, rect: PixelRect, x: number, y: number, pxPerTick: number, gainLine: boolean): AudioZone | null {
  if (clip.content.type !== "audio") return null;
  const nearTop = y >= rect.y && y <= rect.y + FADE_HANDLE_PX * 3;
  for (const edge of ["in", "out"] as const) {
    if (nearTop && Math.abs(x - fadeHandleX(clip, rect, edge, pxPerTick)) <= FADE_HANDLE_PX * 1.5) return { kind: "fade", edge };
  }
  return gainLine && Math.abs(y - gainLineY(rect, clip.gain)) <= GAIN_GRAB_PX ? { kind: "gain" } : null;
}
