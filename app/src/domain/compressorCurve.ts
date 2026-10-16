// The compressor's static transfer curve (input level → output level) for its graph (`dynamicsGraph.ts`). The curve
// MIRRORS `Compressor::target_reduction` in `crates/daw-effects/src/compressor.rs` (threshold, ratio, soft knee; makeup
// on top): a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "./catalog";
import type { DynamicsCurve } from "./dynamicsGraph";

export interface CompressorSettings {
  threshold: number;
  ratio: number;
  knee: number;
  makeup: number;
}

/** Gain reduction (dB, 0 or more) for input level `levelDb`. */
export function targetReduction(s: CompressorSettings, levelDb: number): number {
  const over = levelDb - s.threshold;
  const slope = 1 - 1 / s.ratio;
  if (2 * over < -s.knee) return 0;
  if (2 * Math.abs(over) <= s.knee) return (slope * (over + s.knee / 2) ** 2) / (2 * Math.max(s.knee, Number.EPSILON));
  return slope * over;
}

/** The output level for input `levelDb`: reduced, then raised by makeup. */
export const outputDb = (s: CompressorSettings, levelDb: number) => levelDb - targetReduction(s, levelDb) + s.makeup;

/** The curve by the compressor's param names; `null` when one is missing. */
export function compressorCurve(infos: ParamInfo[], values: Params): DynamicsCurve | null {
  const s = namedParams(infos, values, ["threshold", "ratio", "knee", "makeup"]);
  return s && { branches: [(level) => outputDb(s, level)], thresholds: [s.threshold] };
}
