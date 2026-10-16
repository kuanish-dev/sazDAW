// Draws the sample waveform on canvas: time ruler, waveform, selection, slice markers, cursors. Only draws what it is given.
import type { Waveform } from "../../api/types";
import { formatTime, rulerStep, xAtFrame, type Selection, type View } from "../../domain/waveform";
import { fitCanvas, vline } from "../common/canvas";
import { palette, type Palette } from "../common/theme";

export const RULER_HEIGHT = 24;
export const WAVE_HEIGHT = 250;
/** Waveform padding from the lane edges, pixels. */
const WAVE_PAD = 6;
/** A marker is drawn while it is at least near the visible part. */
const MARKER_MARGIN = 10;
const FLAG_HALF_WIDTH = 6;

export interface WaveformView {
  width: number;
  wave: Waveform | null;
  sampleRate: number;
  view: View;
  selection: Selection | null;
  markers: number[];
  /** Frame under the mouse (thin line) and audition frame (cursor); `null` means none. */
  hover: number | null;
  playhead: number | null;
}

function drawRuler(ctx: CanvasRenderingContext2D, v: WaveformView, P: Palette) {
  ctx.fillStyle = P.rowA;
  ctx.fillRect(0, 0, v.width, RULER_HEIGHT);
  const stepFrames = rulerStep(v.view.span, v.sampleRate, v.width) * v.sampleRate;
  ctx.font = `10px ${P.mono}`;
  ctx.textBaseline = "middle";
  for (let t = Math.ceil(v.view.start / stepFrames) * stepFrames; t <= v.view.start + v.view.span; t += stepFrames) {
    const x = xAtFrame(t, v.width, v.view);
    vline(ctx, x, RULER_HEIGHT - 7, WAVE_HEIGHT, P.grid);
    ctx.fillStyle = P.text;
    ctx.fillText(formatTime(t, v.sampleRate), Math.round(x) + 3.5, 9);
  }
}

/** Center line and envelope bars per bucket. */
function drawWave(ctx: CanvasRenderingContext2D, v: WaveformView, P: Palette) {
  const mid = RULER_HEIGHT + (WAVE_HEIGHT - RULER_HEIGHT) / 2;
  const half = (WAVE_HEIGHT - RULER_HEIGHT) / 2 - WAVE_PAD;
  ctx.strokeStyle = P.beat;
  ctx.beginPath();
  ctx.moveTo(0, mid + 0.5);
  ctx.lineTo(v.width, mid + 0.5);
  ctx.stroke();

  const wave = v.wave;
  if (!wave || wave.peaks.length === 0) return;
  ctx.fillStyle = P.accent;
  const count = wave.peaks.length;
  const barWidth = Math.max(1, ((wave.to - wave.from) / count / v.view.span) * v.width);
  for (let i = 0; i < count; i++) {
    const x = xAtFrame(wave.from + ((wave.to - wave.from) * i) / count, v.width, v.view);
    const [low, high] = wave.peaks[i]!;
    const top = mid - Math.min(1, high) * half;
    const bottom = mid - Math.max(-1, low) * half;
    ctx.fillRect(x, top, barWidth, Math.max(1, bottom - top));
  }
}

function drawSelection(ctx: CanvasRenderingContext2D, v: WaveformView, P: Palette) {
  if (!v.selection) return;
  const x0 = xAtFrame(v.selection.from, v.width, v.view);
  const x1 = xAtFrame(v.selection.to, v.width, v.view);
  ctx.fillStyle = P.accentSoft;
  ctx.fillRect(x0, RULER_HEIGHT, x1 - x0, WAVE_HEIGHT - RULER_HEIGHT);
  ctx.fillStyle = P.accent;
  ctx.fillRect(x0 - 1, RULER_HEIGHT, 2, WAVE_HEIGHT - RULER_HEIGHT);
  ctx.fillRect(x1 - 1, RULER_HEIGHT, 2, WAVE_HEIGHT - RULER_HEIGHT);
}

/** Slice markers: a dashed line through the waveform and numbered flags on the ruler. */
function drawMarkers(ctx: CanvasRenderingContext2D, v: WaveformView, P: Palette) {
  const visible = v.markers
    .map((marker, i) => ({ x: Math.round(xAtFrame(marker, v.width, v.view)), number: i + 1 }))
    .filter(({ x }) => x >= -MARKER_MARGIN && x <= v.width + MARKER_MARGIN);
  ctx.setLineDash([4, 4]);
  for (const { x } of visible) vline(ctx, x, RULER_HEIGHT, WAVE_HEIGHT, P.blue);
  ctx.setLineDash([]);
  ctx.font = `bold 8px ${P.font}`;
  ctx.textAlign = "center";
  for (const { x, number } of visible) {
    ctx.fillStyle = P.blue;
    ctx.beginPath();
    ctx.moveTo(x, RULER_HEIGHT - 1);
    ctx.lineTo(x - FLAG_HALF_WIDTH, 2);
    ctx.lineTo(x + FLAG_HALF_WIDTH, 2);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = P.bgDeep;
    ctx.fillText(String(number), x, 8);
  }
  ctx.textAlign = "start";
}

function drawCursors(ctx: CanvasRenderingContext2D, v: WaveformView, P: Palette) {
  if (v.hover !== null) vline(ctx, xAtFrame(v.hover, v.width, v.view), RULER_HEIGHT, WAVE_HEIGHT, P.inkSoft);
  if (v.playhead !== null) vline(ctx, xAtFrame(v.playhead, v.width, v.view), 0, WAVE_HEIGHT, P.ink, 1.5);
}

export function drawWaveform(canvas: HTMLCanvasElement, v: WaveformView) {
  const P = palette();
  const ctx = fitCanvas(canvas, v.width, WAVE_HEIGHT);
  ctx.fillStyle = P.bg;
  ctx.fillRect(0, 0, v.width, WAVE_HEIGHT);
  drawRuler(ctx, v, P);
  drawWave(ctx, v, P);
  drawSelection(ctx, v, P);
  drawMarkers(ctx, v, P);
  drawCursors(ctx, v, P);
}
