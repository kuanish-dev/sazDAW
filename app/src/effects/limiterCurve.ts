// The limiter's static transfer curve for its graph (`dynamicsCurves.ts`): the input raised by the input gain, held at
// the ceiling. MIRRORS `Limiter::process` in `crates/daw-effects/src/limiter.rs` (input gain, then a hard ceiling with
// an instant attack): a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import type { DynamicsCurve } from "./dynamicsCurves";

/** The curve by the limiter's param names; `null` when one is missing. Its threshold is the input level that the input
 *  gain lifts to the ceiling: from there on the limiter holds the level. */
export function limiterCurve(infos: ParamInfo[], values: Params): DynamicsCurve | null {
  const s = namedParams(infos, values, ["ceiling", "input"]);
  return s && { branches: [(level) => Math.min(level + s.input, s.ceiling)], thresholds: [s.ceiling - s.input] };
}
