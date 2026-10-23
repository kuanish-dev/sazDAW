import { clamp } from "./math";

/** A graph's SVG view in its own units (the SVG stretches to the panel width). */
export interface PlotView {
  width: number;
  height: number;
}

/** A point in view units. */
export type Point = readonly [number, number];

/** Linear amplitude → dBFS; silence is very low (−180), so a caller's floor applies. */
export const toDb = (amplitude: number) => 20 * Math.log10(Math.max(Math.abs(amplitude), 1e-9));

/** Where a level sits on a dB scale from `floorDb` (0) to `topDb` (1), clamped. */
export const dbFraction = (db: number, floorDb: number, topDb = 0) => clamp((db - floorDb) / (topDb - floorDb), 0, 1);

/** The y of a level in a view whose top is `topDb` and bottom `floorDb`. */
export const dbY = (view: PlotView, db: number, floorDb: number, topDb = 0) => view.height * (1 - dbFraction(db, floorDb, topDb));

const xy = ([x, y]: Point) => `${x.toFixed(1)} ${y.toFixed(1)}`;

/** An open line through the points, broken where a point is `null` (a level below the floor of its graph); empty when
 *  there is nothing to draw. */
export function linePath(points: readonly (Point | null)[]): string {
  let path = "";
  let drawing = false;
  for (const point of points) {
    if (point === null) {
      drawing = false;
      continue;
    }
    path += `${drawing ? "L" : "M"}${xy(point)} `;
    drawing = true;
  }
  return path.trimEnd();
}

/** The points as a closed area down to `baseY` (the bottom of the view for a level graph); empty for no points. */
export function areaPath(points: readonly Point[], baseY: number): string {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return "";
  return `M${first[0].toFixed(1)} ${baseY} ${points.map((p) => `L${xy(p)}`).join(" ")} L${last[0].toFixed(1)} ${baseY} Z`;
}
