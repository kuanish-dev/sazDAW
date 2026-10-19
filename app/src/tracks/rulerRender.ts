// Draws the ruler strip above the timeline rows on its own canvas: bar numbers and beat ticks, the loop,
// signature flags (`tempoRender.ts`), marker flags (`markerRender.ts`) and the cue flag. The strip stays on screen while the
// page scrolls (`TracksView`); like the rows canvas, it covers only the visible window (`window`).
import { BAR_TICKS, type Marker, type TempoMap } from "../api/types";
import { gridLines } from "../common/tempoMap";
import { fitCanvas, vline } from "../common/canvas";
import type { ScrollWindow } from "../common/scrollWindow";
import { palette, type Palette } from "../common/theme";
import { CUE_HALF_WIDTH, CUE_HEIGHT, CUE_TOP } from "./cueLayout";
import { drawMarkerFlags } from "./markerRender";
import { drawSignatureFlags } from "./tempoRender";
import { RULER_HEIGHT } from "./timelineLayout";

/** Height of the loop bar at the bottom of the bar ruler and of beat ticks. */
const LOOP_BAR_HEIGHT = 4;
const BEAT_TICK_HEIGHT = 8;

export interface RulerView {
  /** Tempo map: bars and signature flags. */
  tempoMap: TempoMap;
  /** Section markers (with the drag draft). */
  markers: readonly Marker[];
  loop: [number, number] | null;
  /** The start position (cue) in ticks; `null` — none. */
  cue: number | null;
  pxPerTick: number;
  bars: number;
  /** Visible part of the timeline (pixels from its start). */
  window: ScrollWindow;
}

function drawBars(ctx: CanvasRenderingContext2D, view: RulerView, P: Palette) {
  const { left, width } = view.window;
  ctx.fillStyle = P.bg;
  ctx.fillRect(left, 0, width, RULER_HEIGHT);
  ctx.font = `11px ${P.font}`;
  ctx.textBaseline = "middle";
  const last = Math.min(view.bars * BAR_TICKS, (left + width) / view.pxPerTick);
  for (const line of gridLines(view.tempoMap, left / view.pxPerTick, last)) {
    const x = line.tick * view.pxPerTick;
    const isBar = line.bar !== null;
    vline(ctx, x, isBar ? 0 : RULER_HEIGHT - BEAT_TICK_HEIGHT, RULER_HEIGHT, isBar ? P.bar : P.grid);
    if (isBar) {
      ctx.fillStyle = P.text;
      ctx.fillText(String(line.bar), Math.round(x) + 4.5, 10);
    }
  }
}

function drawLoop(ctx: CanvasRenderingContext2D, loop: [number, number], pxPerTick: number, P: Palette) {
  const x = loop[0] * pxPerTick;
  const w = (loop[1] - loop[0]) * pxPerTick;
  ctx.fillStyle = P.selectFill;
  ctx.fillRect(x, 0, w, RULER_HEIGHT);
  ctx.fillStyle = P.blue;
  ctx.fillRect(x, RULER_HEIGHT - LOOP_BAR_HEIGHT, w, LOOP_BAR_HEIGHT);
}

/** The cue flag: a downward triangle pointing at the cue tick. */
function drawCue(ctx: CanvasRenderingContext2D, cue: number, pxPerTick: number, P: Palette) {
  const x = Math.round(cue * pxPerTick) + 0.5;
  ctx.fillStyle = P.ink;
  ctx.beginPath();
  ctx.moveTo(x - CUE_HALF_WIDTH, CUE_TOP);
  ctx.lineTo(x + CUE_HALF_WIDTH, CUE_TOP);
  ctx.lineTo(x, CUE_TOP + CUE_HEIGHT);
  ctx.closePath();
  ctx.fill();
}

export function drawRuler(canvas: HTMLCanvasElement, view: RulerView) {
  const P = palette();
  const ctx = fitCanvas(canvas, view.window.width, RULER_HEIGHT);
  ctx.translate(-view.window.left, 0);
  drawBars(ctx, view, P);
  drawSignatureFlags(ctx, view.tempoMap, view, P);
  if (view.loop) drawLoop(ctx, view.loop, view.pxPerTick, P);
  drawMarkerFlags(ctx, view.markers, view, P);
  if (view.cue !== null) drawCue(ctx, view.cue, view.pxPerTick, P);
}
