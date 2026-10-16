// The gate's static transfer curve for its graph (`dynamicsGraph.ts`) and whether it is open now. MIRRORS `Gate` in
// `crates/daw-effects/src/gate.rs` (opens at the threshold, closes `HYSTERESIS_DB` below it, a closed gate lowers the
// sound by the depth `range`): a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "./catalog";
import type { DynamicsCurve } from "./dynamicsGraph";

/** How far below the threshold the level must fall for the gate to close (`HYSTERESIS_DB`). */
const HYSTERESIS_DB = 6;

/** The curve by the gate's param names; `null` when one is missing. Two branches: on the way up the gate stays closed
 *  until the threshold, on the way down it stays open until the closing level, so between the two the output depends on
 *  where the sound came from. */
export function gateCurve(infos: ParamInfo[], values: Params): DynamicsCurve | null {
  const s = namedParams(infos, values, ["threshold", "range"]);
  if (!s) return null;
  const close = s.threshold - HYSTERESIS_DB;
  return {
    branches: [(level) => (level < s.threshold ? level + s.range : level), (level) => (level < close ? level + s.range : level)],
    thresholds: [s.threshold, close],
    // Open while it takes away less than half of its depth; a gate with no depth changes nothing and counts as open.
    isOpen: (reductionDb) => s.range === 0 || reductionDb < -s.range / 2,
  };
}
