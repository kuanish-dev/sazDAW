// The graph of the dynamics effects — compressor, limiter, gate (`ui/effects/DynamicsGraph.tsx`): a square transfer
// curve (input level → output level) with the effect's thresholds marked and the live sound as a dot on it, and beside
// it the history of the last seconds: input level, output level and gain reduction against the same thresholds. Each
// effect gives its static curve from its own mirror module (`compressorCurve.ts`, `limiterCurve.ts`, `gateCurve.ts`).
import { clamp } from "./math";
import { type PlotView, dbY } from "./plot";

/** Both axes of the curve and the level axis of the history run from this many dB up to 0 dBFS. */
export const CURVE_FLOOR_DB = -60;
/** Full scale of the gain reduction meter, dB. */
export const REDUCTION_SCALE_DB = 24;
/** How far back the history reaches, seconds. */
export const HISTORY_SECONDS = 3;

/** The static shape of a dynamics effect. */
export interface DynamicsCurve {
  /** Output level for an input level, dB. One branch, or two for a gate: the way up (closed until the threshold) and
   *  the way down (open until the closing level), which differ between those two levels. */
  branches: ((levelDb: number) => number)[];
  /** Input levels where the effect starts to act, dB: the threshold (for a gate also its closing level). */
  thresholds: number[];
  /** A gate's state from its gain reduction: whether it lets the sound through now. */
  isOpen?: (reductionDb: number) => boolean;
}

/** Position (0..1) of a level on the curve axes. */
export const levelFraction = (db: number) => (clamp(db, CURVE_FLOOR_DB, 0) - CURVE_FLOOR_DB) / -CURVE_FLOOR_DB;

/** The y of a level in a view (0 dBFS at the top, `CURVE_FLOOR_DB` at the bottom). */
export const levelY = (view: PlotView, db: number) => dbY(view, db, CURVE_FLOOR_DB);

/** The SVG path of one branch of the curve in a `size × size` square (output clipped to it). */
export function curvePath(branch: (levelDb: number) => number, size: number, points = 120): string {
  const parts: string[] = [];
  for (let i = 0; i <= points; i++) {
    const input = CURVE_FLOOR_DB + (i / points) * -CURVE_FLOOR_DB;
    parts.push(`${i === 0 ? "M" : "L"}${(levelFraction(input) * size).toFixed(1)} ${((1 - levelFraction(branch(input))) * size).toFixed(1)}`);
  }
  return parts.join(" ");
}

/** Fill (0..1) of the gain reduction meter. */
export const reductionFraction = (db: number) => clamp(db / REDUCTION_SCALE_DB, 0, 1);
