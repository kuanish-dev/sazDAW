// Shared canvas helpers: sizing for the screen pixel density, vertical lines (cursors, grid) and the selection box.
import type { Palette } from "./theme";

/** Fits the canvas to `w`×`h` CSS pixels at the screen density; returns a context in CSS pixels. */
export function fitCanvas(canvas: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D {
  const ratio = window.devicePixelRatio || 1;
  // Resize only on a real change: assigning the width clears the canvas and is expensive.
  if (canvas.width !== Math.round(w * ratio) || canvas.height !== Math.round(h * ratio)) {
    canvas.width = Math.round(w * ratio);
    canvas.height = Math.round(h * ratio);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  }
  const ctx = canvas.getContext("2d")!;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  return ctx;
}

/** Vertical line from `top` to `bottom`; the half-pixel offset keeps a thin line sharp. */
export function vline(ctx: CanvasRenderingContext2D, x: number, top: number, bottom: number, color: string, width = 1) {
  const sharp = Math.round(x) + 0.5;
  ctx.strokeStyle = color;
  ctx.lineWidth = width;
  ctx.beginPath();
  ctx.moveTo(sharp, top);
  ctx.lineTo(sharp, bottom);
  ctx.stroke();
  ctx.lineWidth = 1;
}

/** The rubber-band selection box between two corners (in any order): a translucent fill with a sharp outline. */
export function drawSelectionBox(ctx: CanvasRenderingContext2D, x0: number, y0: number, x1: number, y1: number, P: Palette) {
  const [x, y, w, h] = [Math.min(x0, x1), Math.min(y0, y1), Math.abs(x1 - x0), Math.abs(y1 - y0)];
  ctx.fillStyle = P.selectFill;
  ctx.fillRect(x, y, w, h);
  ctx.strokeStyle = P.selectStroke;
  ctx.strokeRect(x + 0.5, y + 0.5, w, h);
}
