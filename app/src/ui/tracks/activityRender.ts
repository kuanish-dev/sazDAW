// Draws what is sounding right now on a transparent canvas laid over the timeline rows: notes under the playhead light up
// in their clip's color and fade out over a sixteenth after they end, and every clip being played gets a glowing outline.
// It is redrawn on every screen frame while playing (from `smoothPlayhead`), so it looks only at the clips under the
// playhead and never touches the heavy rows canvas below it (`timelineRender.ts`).
import { PPQ, type Clip, type Id } from "../../api/types";
import { clipEnd } from "../../domain/clips";
import { type RowLayout, timelineHeight } from "../../domain/rowLayout";
import { laneOf, type Lanes } from "../../domain/trackRows";
import { fitCanvas } from "../common/canvas";
import type { ScrollWindow } from "../common/scrollWindow";
import { CLIP_RADIUS } from "./clipFrame";
import { clipRect, notePreview } from "./timelineLayout";

/** How long a note or clip keeps glowing after it ends, ticks: a sixteenth, short enough to follow fast parts. */
const AFTERGLOW_TICKS = PPQ / 4;
const NOTE_GLOW_PX = 8;
const CLIP_GLOW_PX = 12;
/** A lit note gets a white core over its color, so it reads on any track color. */
const NOTE_CORE = "rgba(255,255,255,0.6)";

export interface ActivityView {
  clips: readonly Clip[];
  lanes: Lanes;
  clipColor: (clip: Clip) => string;
  /** Tracks that are not heard (muted, or silenced by a solo): nothing of theirs lights up. */
  muted: ReadonlySet<Id>;
  pxPerTick: number;
  layout: RowLayout;
  window: ScrollWindow;
  /** Playhead, ticks; `null` — stopped, nothing is lit. */
  position: number | null;
}

/** Glow of a span at `at`: full while it plays, fading out over `fade` after its end, none before its start. */
function glow(start: number, end: number, at: number, fade: number): number {
  if (at < start) return 0;
  if (at < end) return 1;
  return Math.max(0, 1 - (at - end) / fade);
}

/** The clip outline at `strength`, and its notes lit by how close they are to the playhead (`position`, ticks). */
function drawClipGlow(ctx: CanvasRenderingContext2D, view: ActivityView, clip: Clip, row: number, strength: number, position: number) {
  const rect = clipRect(clip, row, view.pxPerTick, view.layout);
  const color = view.clipColor(clip);
  ctx.globalAlpha = strength;
  ctx.shadowColor = color;
  ctx.shadowBlur = CLIP_GLOW_PX;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.roundRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1, CLIP_RADIUS);
  ctx.stroke();

  // Notes are compared in pixels: `notePreview` places them (with the clip's loop repeats) where the rows canvas drew them.
  const at = position * view.pxPerTick;
  const fade = AFTERGLOW_TICKS * view.pxPerTick;
  for (const note of notePreview(clip, rect, view.pxPerTick)) {
    const lit = glow(note.x, note.x + note.w + 1, at, fade);
    if (lit === 0) continue;
    ctx.globalAlpha = lit;
    ctx.shadowBlur = NOTE_GLOW_PX;
    ctx.fillStyle = color;
    ctx.fillRect(note.x, note.y - 1, note.w, note.h + 2);
    ctx.shadowBlur = 0;
    ctx.fillStyle = NOTE_CORE;
    ctx.fillRect(note.x, note.y, note.w, note.h);
  }
}

export function drawActivity(canvas: HTMLCanvasElement, view: ActivityView) {
  const h = timelineHeight(view.layout);
  const ctx = fitCanvas(canvas, view.window.width, h);
  ctx.clearRect(0, 0, view.window.width, h);
  const at = view.position;
  if (at === null) return;
  ctx.translate(-view.window.left, 0);
  for (const clip of view.clips) {
    const strength = glow(clip.startTick, clipEnd(clip), at, AFTERGLOW_TICKS);
    const row = strength > 0 && !view.muted.has(clip.track) ? laneOf(view.lanes, clip.track) : -1;
    if (row >= 0) drawClipGlow(ctx, view, clip, row, strength, at);
  }
  ctx.globalAlpha = 1;
  ctx.shadowBlur = 0;
  ctx.lineWidth = 1;
}
