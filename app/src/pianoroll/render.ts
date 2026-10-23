import type { Note } from "../api/types";
import { inScale, type ScaleChoice } from "./noteTransforms";
import { HIGH_PITCH, LOW_PITCH, MAX_VELOCITY, isBlackKey, pitchName } from "../common/pitch";
import type { TickPitchRect } from "./noteSelection";
import { t } from "../i18n";
import { drawSelectionBox, fitCanvas, vline } from "../common/canvas";
import { palette, type Palette } from "../common/theme";
import { GRID_HEIGHT, HANDLE_PX, KEYS_WIDTH, ROW_HEIGHT, VELOCITY_HEIGHT, rowY, tickX, velocityBarHeight, velocityBarTop } from "./pianoRollLayout";

/** Black key width as a share of the keys width. */
const BLACK_KEY_RATIO = 0.62;
/** Every key is labeled with its name; C keys (octave starts) in bold. */
const WHITE_KEY_INK = "rgba(0,0,0,0.55)";
const BLACK_KEY_INK = "rgba(255,255,255,0.7)";
/** A note is paler at low velocity: from half to full opacity. */
const MIN_NOTE_ALPHA = 0.5;
const NOTE_HANDLE_SHADE = "rgba(0,0,0,0.35)";
const MIN_NOTE_PX = 3;
/** A note shows its name (C4, F#3) once it is wide enough to fit it. */
const NOTE_LABEL_MIN_PX = 22;
const NOTE_LABEL_PAD = 3;
const SELECTED_NOTE_INK = "rgba(0,0,0,0.8)";
/** Spare bar: dashed line and label in the accent color, but muted. */
const GHOST_LINE_ALPHA = 0.45;
const GHOST_TEXT_ALPHA = 0.7;
const VELOCITY_BAR_WIDTH = 4;

/** Everything needed to draw the grid: data and scale, without touching app state. */
export interface GridView {
  width: number;
  pxPerTick: number;
  /** Visible length (clip content plus the spare bar) and the length of the content itself. */
  viewLength: number;
  contentTicks: number;
  lineStep: number;
  /** The clip's bar and beat in ticks: the time signature at the clip start (`pianoroll/noteClip.ts: clipBarTicks`). */
  barTicks: number;
  beatTicks: number;
  notes: Note[];
  selected: Set<number>;
  pressedPitches: Set<number>;
  marquee: TickPitchRect | null;
  /** Playhead position in pixels, or `null`. */
  playheadX: number | null;
  /** Scale rows to highlight, or `null`. */
  scale: ScaleChoice | null;
}

function drawKeysAndRows(ctx: CanvasRenderingContext2D, v: GridView, P: Palette) {
  ctx.textBaseline = "middle";
  for (let pitch = LOW_PITCH; pitch <= HIGH_PITCH; pitch++) {
    const y = rowY(pitch);
    const black = isBlackKey(pitch);
    ctx.fillStyle = black ? P.rowA : P.rowB;
    ctx.fillRect(KEYS_WIDTH, y, v.width - KEYS_WIDTH, ROW_HEIGHT);
    if (v.scale && inScale(pitch, v.scale.root, v.scale.scale)) {
      ctx.fillStyle = P.accentSoft;
      ctx.fillRect(KEYS_WIDTH, y, v.width - KEYS_WIDTH, ROW_HEIGHT);
    }
    ctx.fillStyle = v.pressedPitches.has(pitch) ? P.accent : black ? P.keyBlack : P.keyWhite;
    ctx.fillRect(0, y, black ? KEYS_WIDTH * BLACK_KEY_RATIO : KEYS_WIDTH - 1, ROW_HEIGHT - 1);
    const octave = pitch % 12 === 0;
    ctx.fillStyle = v.pressedPitches.has(pitch) ? P.onAccent : black ? BLACK_KEY_INK : WHITE_KEY_INK;
    ctx.font = `${octave ? 600 : 400} 10px ${P.font}`;
    ctx.fillText(pitchName(pitch), 6, y + ROW_HEIGHT / 2);
    if (octave) {
      ctx.fillStyle = P.grid;
      ctx.fillRect(KEYS_WIDTH, y + ROW_HEIGHT - 1, v.width - KEYS_WIDTH, 1);
    }
  }
}

/** What the grid lines need: steps from the content start, beats stronger, bars on top. */
export type TickGrid = Pick<GridView, "pxPerTick" | "viewLength" | "lineStep" | "beatTicks" | "barTicks">;

/** The grid's vertical lines `height` tall: under the notes and, the same, under the controller lane. */
export function drawTickGrid(ctx: CanvasRenderingContext2D, v: TickGrid, height: number, P: Palette) {
  for (let tick = 0; tick <= v.viewLength; tick += v.lineStep) {
    vline(ctx, tickX(tick, v.pxPerTick), 0, height, tick % v.beatTicks === 0 ? P.beat : P.grid);
  }
  for (let tick = 0; tick <= v.viewLength; tick += v.barTicks) vline(ctx, tickX(tick, v.pxPerTick), 0, height, P.bar);
}

/** The spare bar: translucent and inactive while it has no notes. */
function drawGhostBar(ctx: CanvasRenderingContext2D, v: GridView, P: Palette) {
  if (v.viewLength <= v.contentTicks) return;
  const start = tickX(v.contentTicks, v.pxPerTick);
  ctx.fillStyle = P.dim;
  ctx.fillRect(start, 0, v.width - start, GRID_HEIGHT);
  ctx.globalAlpha = GHOST_LINE_ALPHA;
  ctx.setLineDash([6, 5]);
  vline(ctx, start, 0, GRID_HEIGHT, P.accent);
  ctx.setLineDash([]);
  ctx.globalAlpha = GHOST_TEXT_ALPHA;
  ctx.fillStyle = P.accent;
  ctx.font = `11px ${P.font}`;
  ctx.textBaseline = "top";
  ctx.fillText(t().pianoRoll.ghostBar, start + 8, 6);
  ctx.globalAlpha = 1;
}

function drawNotes(ctx: CanvasRenderingContext2D, v: GridView, P: Palette) {
  for (const n of v.notes) {
    if (n.pitch < LOW_PITCH || n.pitch > HIGH_PITCH) continue;
    const x = tickX(n.startTick, v.pxPerTick);
    const y = rowY(n.pitch);
    const w = Math.max(MIN_NOTE_PX, n.lengthTicks * v.pxPerTick - 1);
    const isSelected = v.selected.has(n.id);
    ctx.globalAlpha = MIN_NOTE_ALPHA + (1 - MIN_NOTE_ALPHA) * (n.velocity / MAX_VELOCITY);
    ctx.fillStyle = isSelected ? P.noteSelected : P.accent;
    ctx.fillRect(x, y + 1, w, ROW_HEIGHT - 2);
    ctx.globalAlpha = 1;
    const handle = Math.min(HANDLE_PX / 2, w / 2);
    ctx.fillStyle = NOTE_HANDLE_SHADE;
    ctx.fillRect(x + w - handle, y + 1, handle, ROW_HEIGHT - 2);
    if (isSelected) {
      ctx.strokeStyle = P.ink;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(x + 0.5, y + 1.5, w - 1, ROW_HEIGHT - 3);
      ctx.lineWidth = 1;
    }    if (w >= NOTE_LABEL_MIN_PX) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w - handle, ROW_HEIGHT);
      ctx.clip();
      ctx.fillStyle = isSelected ? SELECTED_NOTE_INK : P.onAccent;
      ctx.font = `500 10px ${P.font}`;
      ctx.textBaseline = "middle";
      ctx.fillText(pitchName(n.pitch), x + NOTE_LABEL_PAD, y + ROW_HEIGHT / 2);
      ctx.restore();
    }
  }
}

function drawMarquee(ctx: CanvasRenderingContext2D, v: GridView, P: Palette) {
  const box = v.marquee;
  if (!box) return;
  const x0 = tickX(Math.min(box.fromTick, box.toTick), v.pxPerTick);
  const x1 = tickX(Math.max(box.fromTick, box.toTick), v.pxPerTick);
  const y0 = rowY(Math.max(box.fromPitch, box.toPitch));
  const y1 = rowY(Math.min(box.fromPitch, box.toPitch)) + ROW_HEIGHT;
  drawSelectionBox(ctx, x0, y0, x1, y1, P);
}

export function drawGrid(canvas: HTMLCanvasElement, v: GridView) {
  const P = palette();
  const ctx = fitCanvas(canvas, v.width, GRID_HEIGHT);
  ctx.fillStyle = P.bg;
  ctx.fillRect(0, 0, v.width, GRID_HEIGHT);
  drawKeysAndRows(ctx, v, P);
  drawTickGrid(ctx, v, GRID_HEIGHT, P);
  drawGhostBar(ctx, v, P);
  drawNotes(ctx, v, P);
  drawMarquee(ctx, v, P);
  if (v.playheadX !== null) vline(ctx, v.playheadX, 0, GRID_HEIGHT, P.ink);
}

export function drawVelocity(canvas: HTMLCanvasElement, v: Pick<GridView, "width" | "pxPerTick" | "notes" | "selected">) {
  const P = palette();
  const ctx = fitCanvas(canvas, v.width, VELOCITY_HEIGHT);
  ctx.fillStyle = P.bg;
  ctx.fillRect(0, 0, v.width, VELOCITY_HEIGHT);
  ctx.fillStyle = P.text;
  ctx.font = `10px ${P.font}`;
  ctx.textBaseline = "middle";
  ctx.fillText(t().pianoRoll.velocity, 6, VELOCITY_HEIGHT / 2);
  for (const n of v.notes) {
    const x = tickX(n.startTick, v.pxPerTick);
    const top = velocityBarTop(n.velocity);
    ctx.fillStyle = v.selected.has(n.id) ? P.noteSelected : P.accent;
    ctx.fillRect(x, top, VELOCITY_BAR_WIDTH, velocityBarHeight(n.velocity));
    ctx.fillRect(x - 3, top - 2, 10, 4);
  }
}
