// Geometry of section marker flags on the timeline ruler: where a flag is and whether the mouse hits it. Shared by drawing
// (`markerRender.ts`) and mouse (`markerPointer.ts`). Flags sit in the lower half of the ruler: bar numbers use the upper half.
import type { Marker } from "../api/types";

export const MARKER_TOP = 15;
export const MARKER_HEIGHT = 12;
/** Flag width from its name (estimated without a canvas: mouse and drawing must agree). */
const CHAR_PX = 6.5;
const FLAG_PAD = 8;
const MAX_FLAG_PX = 140;

export const flagWidth = (name: string): number => Math.min(MAX_FLAG_PX, FLAG_PAD + name.length * CHAR_PX);

/** Marker under canvas point (x, y); on overlapping flags, the rightmost one (it is drawn on top). */
export function markerAt(markers: readonly Marker[], x: number, y: number, pxPerTick: number): Marker | null {
  if (y < MARKER_TOP || y > MARKER_TOP + MARKER_HEIGHT) return null;
  for (let i = markers.length - 1; i >= 0; i--) {
    const m = markers[i]!;
    const left = m.tick * pxPerTick;
    if (x >= left - 2 && x <= left + flagWidth(m.name)) return m;
  }
  return null;
}
