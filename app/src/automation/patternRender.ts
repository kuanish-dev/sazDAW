import type { AutoPoint, TempoMap } from "../api/types";
import { ratioToValue, valueToRatio, type Range } from "./automationRules";
import { curveShape } from "./automationCurve";
import { gridLines } from "../common/tempoMap";
import { drawSelectionBox, fitCanvas, vline } from "../common/canvas";
import { palette } from "../common/theme";

export const AUTOMATION_HEIGHT = 320;
export const POINT_RADIUS = 5;
/** Radius of a segment's bend handle. */
export const HANDLE_RADIUS = 3;
/** Pixels between samples of a bent segment. */
const CURVE_STEP_PX = 3;
/** Curve padding from the lane's top and bottom so points at the range edges are fully visible. */
const CURVE_PAD = 8;
const CURVE_SPAN = AUTOMATION_HEIGHT - CURVE_PAD * 2;
const CURVE_WIDTH = 2;

/** Y of a point with value `value` in the lane. */
export const yOfValue = (value: number, range: Range): number => AUTOMATION_HEIGHT - CURVE_PAD - valueToRatio(value, range) * CURVE_SPAN;

/** Value at height `y` in the lane (beyond the edges — the range edge). */
export const valueAtY = (y: number, range: Range): number => ratioToValue((AUTOMATION_HEIGHT - CURVE_PAD - y) / CURVE_SPAN, range);

/** Where the bend handle of segment `a → b` is drawn: halfway in time, on the curve. */
export function segmentMiddle(a: AutoPoint, b: AutoPoint, range: Range, pxPerTick: number): { x: number; y: number } {
  const value = a.value + (b.value - a.value) * curveShape(0.5, a.curve);
  return { x: ((a.tick + b.tick) / 2) * pxPerTick, y: yOfValue(value, range) };
}

/** Traces segment `a → b` from the current path point: straight, or sampled along its curve. */
function traceSegment(ctx: CanvasRenderingContext2D, a: AutoPoint, b: AutoPoint, pxPerTick: number, y: (value: number) => number) {
  const x0 = a.tick * pxPerTick;
  const x1 = b.tick * pxPerTick;
  if (!a.curve || x1 - x0 < CURVE_STEP_PX * 2) {
    ctx.lineTo(x1, y(b.value));
    return;
  }
  const steps = Math.ceil((x1 - x0) / CURVE_STEP_PX);
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    ctx.lineTo(x0 + (x1 - x0) * t, y(a.value + (b.value - a.value) * curveShape(t, a.curve)));
  }
}

export interface AutomationView {
  width: number;
  pxPerTick: number;
  /** Tempo map: grid beats and bars as on the timeline. */
  tempoMap: TempoMap;
  /** Song tick of the pattern start (the clip start): the grid is the song's grid there. */
  startTick: number;
  points: AutoPoint[];
  /** Indices of the selected points. */
  selected: ReadonlySet<number>;
  /** Selection rectangle being dragged (pattern pixels); `null` — none. */
  box: { x0: number; y0: number; x1: number; y1: number } | null;
  range: Range;
  playhead: number | null;
}

/** The whole pattern is drawn: it is stretched over the panel width, so nothing of it is ever scrolled away. */
export function drawAutomation(canvas: HTMLCanvasElement, v: AutomationView) {
  const P = palette();
  const ctx = fitCanvas(canvas, v.width, AUTOMATION_HEIGHT);
  ctx.fillStyle = P.bg;
  ctx.fillRect(0, 0, v.width, AUTOMATION_HEIGHT);
  for (const line of gridLines(v.tempoMap, v.startTick, v.startTick + v.width / v.pxPerTick)) {
    vline(ctx, (line.tick - v.startTick) * v.pxPerTick, 0, AUTOMATION_HEIGHT, line.bar !== null ? P.bar : P.grid);
  }
  const points = v.points;
  const y = (value: number) => yOfValue(value, v.range);
  ctx.strokeStyle = P.blue;
  ctx.lineWidth = CURVE_WIDTH;
  ctx.beginPath();
  const first = points[0];
  if (first) {
    ctx.moveTo(0, y(first.value));
    ctx.lineTo(first.tick * v.pxPerTick, y(first.value));
    points.forEach((p, i) => {
      const next = points[i + 1];
      if (next) traceSegment(ctx, p, next, v.pxPerTick, y);
    });
    ctx.lineTo(v.width, y(points[points.length - 1]!.value));
  }
  ctx.stroke();
  ctx.lineWidth = 1;
  ctx.strokeStyle = P.blue;
  points.forEach((p, i) => {
    const next = points[i + 1];
    if (!next) return;
    const middle = segmentMiddle(p, next, v.range, v.pxPerTick);
    ctx.beginPath();
    ctx.arc(middle.x, middle.y, HANDLE_RADIUS, 0, Math.PI * 2);
    ctx.stroke();
  });
  ctx.strokeStyle = P.ink;
  points.forEach((p, i) => {
    const selected = v.selected.has(i);
    ctx.fillStyle = selected ? P.noteSelected : P.blue;
    ctx.beginPath();
    ctx.arc(p.tick * v.pxPerTick, y(p.value), POINT_RADIUS, 0, Math.PI * 2);
    ctx.fill();
    if (selected) ctx.stroke();
  });
  if (v.box) drawSelectionBox(ctx, v.box.x0, v.box.y0, v.box.x1, v.box.y1, P);
  if (v.playhead !== null) vline(ctx, v.playhead * v.pxPerTick, 0, AUTOMATION_HEIGHT, P.ink);
}
