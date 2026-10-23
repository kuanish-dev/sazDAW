import { BAR_TICKS, type Clip, type Id, type Marker, type TempoMap } from "../api/types";
import { clipEnd, crossfades, repeatTicks, type Crossfade } from "./clipRules";
import { fadeGain, type FadeEdge } from "./fades";
import { clamp } from "../common/math";
import { type RowLayout, rowHeight, rowTop, timelineHeight } from "./rowLayout";
import { gridLines } from "../common/tempoMap";
import { laneOf, summaryRowOf, type Lanes, type TrackRow } from "./trackRows";
import { drawSelectionBox, fitCanvas, vline } from "../common/canvas";
import { inWindow, type ScrollWindow } from "../common/scrollWindow";
import { palette, type Palette } from "../common/theme";
import { drawClipFrame } from "./clipFrame";
import { drawLaneRow, type LaneRowView } from "./laneRender";
import { drawMarkerLines } from "./markerRender";
import {
  FADE_HANDLE_PX, HANDLE_PX, clipRect, fadeHandleX, gainLineY, notePreview, showsGainLine, spanRect, type PixelRect,
} from "./timelineLayout";

/** Clip content (notes, waveform) is in the track color, inside the shared clip frame (`clipFrame.ts`). */
const CLIP_SHADE = "rgba(0,0,0,0.3)";
const CLIP_HANDLE = "rgba(255,255,255,0.12)";
const CLIP_BODY_ALPHA = 0.22;
/** Crossfade: darkened overlap and an "X" made of the fade-out and fade-in curves. */
const CROSSFADE_FILL = "rgba(0,0,0,0.28)";
const CROSSFADE_LINE = "rgba(255,255,255,0.9)";
/** Fades: the silenced part above the curve is shaded, the curve and the handles are drawn on top. */
const FADE_SHADE = "rgba(0,0,0,0.3)";
const FADE_LINE = "rgba(255,255,255,0.9)";
const GAIN_LINE = "rgba(255,255,255,0.6)";
/** Curve resolution: one segment per this many pixels. */
const FADE_STEP_PX = 2;
const MUTED_CLIP_ALPHA = 0.35;
/** Collapsed folder clip summary: a strip in the lower part of its row. */
const SUMMARY_ALPHA = 0.6;
const SUMMARY_TOP = 0.55;
const SUMMARY_HEIGHT = 0.3;
/** Space at the top of the clip for the name: the waveform is drawn below it. */
const WAVE_TOP = 16;
/** The cue line is dashed: it must not be mistaken for the playhead. */
const CUE_DASH = [4, 3] as const;

export interface TimelineView {
  /** Rows top to bottom (tracks and folders), plus the same as track-or-`null` lanes for finding a clip's row. */
  rows: TrackRow[];
  lanes: Lanes;
  clips: Clip[];
  /** Clip color (from its track's color). */
  clipColor: (clip: Clip) => string;
  /** Tempo map: grid bars. */
  tempoMap: TempoMap;
  /** Section markers (with the drag draft): their lines through the rows. */
  markers: readonly Marker[];
  selected: Set<Id>;
  /** Marquee in canvas pixels. */
  marquee: PixelRect | null;
  loop: [number, number] | null;
  /** The start position (cue) in ticks; `null` — none. */
  cue: number | null;
  pxPerTick: number;
  /** Where every row is (rows differ in height: a track's own or the appearance setting). */
  layout: RowLayout;
  bars: number;
  mutedTracks: Set<Id>;
  clipName: (clip: Clip) => string;
  /** Waveform columns of an audio clip `width` pixels wide; `null` if there's no waveform yet. */
  wave: (clip: Clip, width: number) => [number, number][] | null;
  /** Visible part of the timeline (pixels from its start). */
  window: ScrollWindow;
  /** Automation lane rows with their clips. */
  automation: readonly LaneRowView[];
}

export const timelineWidth = (view: Pick<TimelineView, "bars" | "pxPerTick">) => view.bars * BAR_TICKS * view.pxPerTick;

function drawGrid(ctx: CanvasRenderingContext2D, view: TimelineView, h: number, P: Palette) {
  const { left, width } = view.window;
  ctx.fillStyle = P.bg;
  ctx.fillRect(left, 0, width, h);
  view.rows.forEach((row, i) => {
    ctx.fillStyle = row.type === "folder" || row.type === "busLanes" ? P.bgDeep : i % 2 === 0 ? P.rowA : P.rowB;
    ctx.fillRect(left, rowTop(i, view.layout), width, rowHeight(i, view.layout));
  });
  const last = Math.min(view.bars * BAR_TICKS, (left + width) / view.pxPerTick);
  for (const line of gridLines(view.tempoMap, left / view.pxPerTick, last)) {
    vline(ctx, line.tick * view.pxPerTick, 0, h, line.bar !== null ? P.bar : P.grid);
  }
}

function drawLoop(ctx: CanvasRenderingContext2D, loop: [number, number], pxPerTick: number, h: number, P: Palette) {
  ctx.fillStyle = P.selectFill;
  ctx.fillRect(loop[0] * pxPerTick, 0, (loop[1] - loop[0]) * pxPerTick, h);
}

function drawCueLine(ctx: CanvasRenderingContext2D, cue: number, pxPerTick: number, h: number, P: Palette) {
  ctx.setLineDash(CUE_DASH);
  vline(ctx, cue * pxPerTick, 0, h, P.inkSoft);
  ctx.setLineDash([]);
}

/** Audio clip waveform: one vertical stroke per pixel, below the name; the clip's middle is the zero line. */
function drawWave(ctx: CanvasRenderingContext2D, columns: [number, number][] | null, rect: PixelRect, win: ScrollWindow, color: string) {
  if (!columns) return;
  const half = (rect.h - WAVE_TOP - 2) / 2;
  const mid = rect.y + WAVE_TOP + half;
  ctx.fillStyle = color;
  const from = Math.max(0, Math.floor(win.left - rect.x));
  const to = Math.min(columns.length, Math.ceil(win.left + win.width - rect.x));
  for (let i = from; i < to; i++) {
    const [low, high] = columns[i]!;
    const y1 = mid - clamp(high, 0, 1) * half;
    const y2 = mid - clamp(low, -1, 0) * half;
    ctx.fillRect(rect.x + i, y1, 1, Math.max(1, y2 - y1));
  }
}

function drawClip(ctx: CanvasRenderingContext2D, view: TimelineView, clip: Clip, row: number, P: Palette) {
  const rect = clipRect(clip, row, view.pxPerTick, view.layout);
  if (!inWindow(rect.x, rect.w, view.window)) return;
  const color = view.clipColor(clip);
  const look = {
    color, name: view.clipName(clip), selected: view.selected.has(clip.id), bodyAlpha: CLIP_BODY_ALPHA,
    alpha: view.mutedTracks.has(clip.track) ? MUTED_CLIP_ALPHA : 1,
  };
  drawClipFrame(ctx, rect, look, P, () => {
    const { content } = clip;
    if (content.type === "audio") drawWave(ctx, view.wave(clip, rect.w), rect, view.window, color);
    ctx.fillStyle = color;
    for (const note of notePreview(clip, rect, view.pxPerTick)) {
      if (inWindow(note.x, note.w, view.window)) ctx.fillRect(note.x, note.y, note.w, note.h);
    }
    if (content.type === "notes") {
      ctx.fillStyle = CLIP_SHADE;
      for (const tick of repeatTicks(clip.startTick, clipEnd(clip), content.loopTicks)) ctx.fillRect(tick * view.pxPerTick, rect.y, 1, rect.h);
    }
    if (content.type === "audio") drawAudioHandles(ctx, view, clip, rect);
    ctx.fillStyle = CLIP_HANDLE;
    ctx.fillRect(rect.x, rect.y, HANDLE_PX / 2, rect.h);
    ctx.fillRect(rect.x + rect.w - HANDLE_PX / 2, rect.y, HANDLE_PX / 2, rect.h);
  });
}

/** A fade of the clip in its shape: the shaded part above the curve and the curve itself (gain 0 at the clip edge). */
function drawFade(ctx: CanvasRenderingContext2D, clip: Clip, rect: PixelRect, edge: FadeEdge, pxPerTick: number) {
  const ticks = edge === "in" ? clip.fadeInTicks : clip.fadeOutTicks;
  const width = Math.min(rect.w, ticks * pxPerTick);
  if (width < 1) return;
  const shape = edge === "in" ? clip.fadeInShape : clip.fadeOutShape;
  const steps = Math.max(2, Math.ceil(width / FADE_STEP_PX));
  const curve = Array.from({ length: steps + 1 }, (_, i) => {
    const progress = i / steps;
    const x = edge === "in" ? rect.x + progress * width : rect.x + rect.w - progress * width;
    return [x, rect.y + rect.h - fadeGain(shape, progress) * rect.h] as const;
  });
  const trace = () => {
    ctx.beginPath();
    ctx.moveTo(curve[0]![0], curve[0]![1]);
    for (const [x, y] of curve) ctx.lineTo(x, y);
  };
  trace();
  ctx.lineTo(curve[steps]![0], rect.y);
  ctx.lineTo(curve[0]![0], rect.y);
  ctx.fillStyle = FADE_SHADE;
  ctx.fill();
  trace();
  ctx.strokeStyle = FADE_LINE;
  ctx.stroke();
}

/** Fade curves, fade handles at the top corners and the gain line (shown when selected or not at 0 dB). */
function drawAudioHandles(ctx: CanvasRenderingContext2D, view: TimelineView, clip: Clip, rect: PixelRect) {
  drawFade(ctx, clip, rect, "in", view.pxPerTick);
  drawFade(ctx, clip, rect, "out", view.pxPerTick);
  ctx.fillStyle = FADE_LINE;
  for (const edge of ["in", "out"] as const) {
    const x = fadeHandleX(clip, rect, edge, view.pxPerTick);
    ctx.fillRect(x - FADE_HANDLE_PX, rect.y, FADE_HANDLE_PX * 2, FADE_HANDLE_PX * 2);
  }
  if (!showsGainLine(clip, view.selected.has(clip.id))) return;
  ctx.fillStyle = GAIN_LINE;
  ctx.fillRect(rect.x, Math.round(gainLineY(rect, clip.gain)), rect.w, 1);
}

/** A clip of a track inside a collapsed folder, drawn as a strip of its color on the folder row. */
function drawSummary(ctx: CanvasRenderingContext2D, view: TimelineView, clip: Clip, row: number) {
  const x = clip.startTick * view.pxPerTick;
  const w = Math.max(2, clip.lengthTicks * view.pxPerTick - 1);
  if (!inWindow(x, w, view.window)) return;
  ctx.globalAlpha = SUMMARY_ALPHA;
  ctx.fillStyle = view.clipColor(clip);
  const h = rowHeight(row, view.layout);
  ctx.fillRect(x, rowTop(row, view.layout) + h * SUMMARY_TOP, w, h * SUMMARY_HEIGHT);
  ctx.globalAlpha = 1;
}

function drawCrossfade(ctx: CanvasRenderingContext2D, view: TimelineView, fade: Crossfade, row: number) {
  const r = spanRect(fade.start, fade.end - fade.start, row, view.pxPerTick, view.layout);
  if (!inWindow(r.x, r.w, view.window)) return;
  ctx.fillStyle = CROSSFADE_FILL;
  ctx.fillRect(r.x, r.y, r.w, r.h);
  ctx.strokeStyle = CROSSFADE_LINE;
  ctx.beginPath();
  ctx.moveTo(r.x, r.y);
  ctx.lineTo(r.x + r.w, r.y + r.h);
  ctx.moveTo(r.x, r.y + r.h);
  ctx.lineTo(r.x + r.w, r.y);
  ctx.stroke();
}

export function drawTimeline(canvas: HTMLCanvasElement, view: TimelineView) {
  const P = palette();
  const h = timelineHeight(view.layout);
  const ctx = fitCanvas(canvas, view.window.width, h);
  ctx.translate(-view.window.left, 0);
  drawGrid(ctx, view, h, P);
  if (view.loop) drawLoop(ctx, view.loop, view.pxPerTick, h, P);
  for (const clip of view.clips) {
    const row = laneOf(view.lanes, clip.track);
    if (row >= 0) drawClip(ctx, view, clip, row, P);
    else {
      const summary = summaryRowOf(view.rows, clip.track);
      if (summary >= 0) drawSummary(ctx, view, clip, summary);
    }
  }
  for (const lane of view.automation) drawLaneRow(ctx, lane, P);
  for (const fade of crossfades(view.clips)) {
    const row = laneOf(view.lanes, fade.track);
    if (row >= 0) drawCrossfade(ctx, view, fade, row);
  }
  drawMarkerLines(ctx, view.markers, { pxPerTick: view.pxPerTick, height: h, window: view.window }, P);
  if (view.cue !== null) drawCueLine(ctx, view.cue, view.pxPerTick, h, P);
  const m = view.marquee;
  if (m) drawSelectionBox(ctx, m.x, m.y, m.x + m.w, m.y + m.h, P);
}
