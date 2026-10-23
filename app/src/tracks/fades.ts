import type { Clip, FadeShape } from "../api/types";
import { clamp } from "../common/math";

/** Shapes in menu order. */
export const FADE_SHAPES: readonly FadeShape[] = ["linear", "equalPower", "sCurve", "exponential"] as const;

/** Steepness of the exponential shape: `(e^(k·x) − 1) / (e^k − 1)`. */
const EXPONENTIAL_STEEPNESS = 5;

/** Gain at `progress` (0 = the clip edge, silent; 1 = full level) through a fade of this shape. */
export function fadeGain(shape: FadeShape, progress: number): number {
  const x = clamp(progress, 0, 1);
  switch (shape) {
    case "equalPower": return Math.sin((x * Math.PI) / 2);
    case "sCurve": return 0.5 - 0.5 * Math.cos(x * Math.PI);
    case "exponential": return Math.expm1(EXPONENTIAL_STEEPNESS * x) / Math.expm1(EXPONENTIAL_STEEPNESS);
    default: return x;
  }
}

export type FadeEdge = "in" | "out";

/** The fade `tick` is inside (the clip's own ticks: a right-click on it offers that fade's shapes); `null` — neither. */
export function fadeAt(clip: Clip, tick: number): FadeEdge | null {
  if (tick < clip.startTick + clip.fadeInTicks) return "in";
  return tick >= clip.startTick + clip.lengthTicks - clip.fadeOutTicks ? "out" : null;
}

/** Fade length when its handle is dropped at `tick` (already snapped): from the clip edge, within the clip. */
export function fadeLengthAt(clip: Clip, edge: FadeEdge, tick: number): number {
  const length = edge === "in" ? tick - clip.startTick : clip.startTick + clip.lengthTicks - tick;
  return Math.round(clamp(length, 0, clip.lengthTicks));
}
