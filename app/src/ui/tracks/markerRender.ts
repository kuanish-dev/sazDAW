// Draws section markers: a named flag on the ruler (`drawMarkerFlags`, ruler canvas) and a thin start line through all
// rows (`drawMarkerLines`, rows canvas). Kept apart from the grid and ruler so signature and tempo flags have their own
// drawing functions.
import type { Marker } from "../../api/types";
import { vline } from "../common/canvas";
import { inWindow, type ScrollWindow } from "../common/scrollWindow";
import type { Palette } from "../common/theme";
import { MARKER_HEIGHT, MARKER_TOP, flagWidth } from "./markerLayout";
import { RULER_HEIGHT } from "./timelineLayout";

const LINE_ALPHA = 0.5;

interface MarkerView {
  pxPerTick: number;
  window: ScrollWindow;
}

/** The flag's left edge, also where its line runs (on both canvases, so the line continues across them). */
const markerX = (marker: Marker, pxPerTick: number) => Math.round(marker.tick * pxPerTick) + 0.5;

const visibleMarkers = (markers: readonly Marker[], o: MarkerView) =>
  markers.filter((marker) => inWindow(marker.tick * o.pxPerTick, flagWidth(marker.name), o.window));

export function drawMarkerFlags(ctx: CanvasRenderingContext2D, markers: readonly Marker[], o: MarkerView, P: Palette) {
  ctx.font = `600 10px ${P.font}`;
  ctx.textBaseline = "middle";
  for (const marker of visibleMarkers(markers, o)) {
    const x = markerX(marker, o.pxPerTick);
    const w = flagWidth(marker.name);
    ctx.globalAlpha = LINE_ALPHA;
    vline(ctx, x, MARKER_TOP, RULER_HEIGHT, P.accent);
    ctx.globalAlpha = 1;
    ctx.fillStyle = P.accent;
    ctx.beginPath();
    ctx.roundRect(x - 0.5, MARKER_TOP, w, MARKER_HEIGHT, [0, 3, 3, 0]);
    ctx.fill();
    ctx.save();
    ctx.beginPath();
    ctx.rect(x, MARKER_TOP, w - 2, MARKER_HEIGHT);
    ctx.clip();
    ctx.fillStyle = P.bg;
    ctx.fillText(marker.name, x + 4, MARKER_TOP + MARKER_HEIGHT / 2 + 0.5);
    ctx.restore();
  }
}

export function drawMarkerLines(ctx: CanvasRenderingContext2D, markers: readonly Marker[], o: MarkerView & { height: number }, P: Palette) {
  ctx.globalAlpha = LINE_ALPHA;
  for (const marker of visibleMarkers(markers, o)) vline(ctx, markerX(marker, o.pxPerTick), 0, o.height, P.accent);
  ctx.globalAlpha = 1;
}
