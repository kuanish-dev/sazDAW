import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import type { DynamicsCurve } from "./dynamicsCurves";

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
    isOpen: (reductionDb) => s.range === 0 || reductionDb < -s.range / 2,
  };
}
