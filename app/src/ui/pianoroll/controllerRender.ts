// Draws the piano roll controller lane on canvas: bar grid, a stepped line (each value holds until the next
// point, the last one until the end of the content) and points. Only draws what it is given (`ControllerView`).
import type { ControllerKind, ControllerPoint } from "../../api/types";
import { controllerRange, formatControllerValue, toFraction } from "../../domain/controllers";
import { fitCanvas } from "../common/canvas";
import { palette, type Palette } from "../common/theme";
import { CONTROLLER_HEIGHT, KEYS_WIDTH, controllerY, tickX } from "./pianoRollLayout";
import { drawTickGrid } from "./render";

const FILL_ALPHA = 0.25;
const POINT_PX = 3;

export interface ControllerView {
  width: number;
  pxPerTick: number;
  /** Visible length (with the spare bar) and content length: the last point holds until the content ends. */
  viewLength: number;
  contentTicks: number;
  lineStep: number;
  barTicks: number;
  beatTicks: number;
  kind: ControllerKind;
  points: ControllerPoint[];
}

function drawGridLines(ctx: CanvasRenderingContext2D, v: ControllerView, P: Palette) {
  drawTickGrid(ctx, v, CONTROLLER_HEIGHT, P);
  if (v.viewLength > v.contentTicks) {
    const start = tickX(v.contentTicks, v.pxPerTick);
    ctx.fillStyle = P.dim;
    ctx.fillRect(start, 0, v.width - start, CONTROLLER_HEIGHT);
  }
}

/** Edge and middle labels on the left, under the keys column. */
function drawScale(ctx: CanvasRenderingContext2D, v: ControllerView, P: Palette) {
  const [min, max] = controllerRange(v.kind);
  ctx.fillStyle = P.bgDeep;
  ctx.fillRect(0, 0, KEYS_WIDTH, CONTROLLER_HEIGHT);
  ctx.fillStyle = P.text;
  ctx.font = `10px ${P.font}`;
  ctx.textBaseline = "middle";
  for (const value of [max, (min + max) / 2, min]) {
    ctx.fillText(formatControllerValue(v.kind, value), 6, controllerY(toFraction(v.kind, value)));
  }
}

function drawLane(ctx: CanvasRenderingContext2D, v: ControllerView, P: Palette) {
  if (v.points.length === 0) return;
  // Fill from the "rest" value: the middle for pitch bend, the bottom for everything else.
  const restY = controllerY(v.kind.type === "pitchBend" ? 0.5 : 0);
  const y = (p: ControllerPoint) => controllerY(toFraction(v.kind, p.value));
  const end = tickX(Math.max(v.contentTicks, v.points.at(-1)!.tick), v.pxPerTick);
  ctx.beginPath();
  ctx.moveTo(tickX(v.points[0]!.tick, v.pxPerTick), restY);
  v.points.forEach((p, i) => {
    const next = v.points[i + 1];
    ctx.lineTo(tickX(p.tick, v.pxPerTick), y(p));
    ctx.lineTo(next ? tickX(next.tick, v.pxPerTick) : end, y(p));
  });
  ctx.lineTo(end, restY);
  ctx.closePath();
  ctx.globalAlpha = FILL_ALPHA;
  ctx.fillStyle = P.accent;
  ctx.fill();
  ctx.globalAlpha = 1;

  ctx.strokeStyle = P.accent;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  v.points.forEach((p, i) => {
    const next = v.points[i + 1];
    if (i === 0) ctx.moveTo(tickX(p.tick, v.pxPerTick), y(p));
    else ctx.lineTo(tickX(p.tick, v.pxPerTick), y(p));
    ctx.lineTo(next ? tickX(next.tick, v.pxPerTick) : end, y(p));
  });
  ctx.stroke();
  ctx.lineWidth = 1;

  ctx.fillStyle = P.ink;
  for (const p of v.points) ctx.fillRect(tickX(p.tick, v.pxPerTick) - POINT_PX / 2, y(p) - POINT_PX / 2, POINT_PX, POINT_PX);
}

export function drawController(canvas: HTMLCanvasElement, v: ControllerView) {
  const P = palette();
  const ctx = fitCanvas(canvas, v.width, CONTROLLER_HEIGHT);
  ctx.fillStyle = P.bg;
  ctx.fillRect(0, 0, v.width, CONTROLLER_HEIGHT);
  drawGridLines(ctx, v, P);
  if (v.kind.type === "pitchBend") {
    ctx.fillStyle = P.grid;
    ctx.fillRect(KEYS_WIDTH, Math.round(controllerY(0.5)), v.width - KEYS_WIDTH, 1);
  }
  drawLane(ctx, v, P);
  drawScale(ctx, v, P);
}
