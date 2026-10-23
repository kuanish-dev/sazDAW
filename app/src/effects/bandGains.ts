import { type Accessor, createEffect, createSignal, on } from "solid-js";
import type { Id } from "../api/types";
import { bandGainsOf, holdBandGains } from "./bandGainRules";
import { usePlayback } from "../core/roles";

/** Longest step between ticks, seconds: after a pause the shown gains do not fall to 0 dB in one jump. */
const MAX_STEP_SECONDS = 0.2;

export function createBandGains(node: () => Id): Accessor<number[]> {
  const { tick } = usePlayback();
  const [gains, setGains] = createSignal<number[]>([]);
  let last = performance.now();
  createEffect(
    on(tick, (tick) => {
      const now = performance.now();
      const dt = Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
      last = now;
      setGains((shown) => holdBandGains(shown, bandGainsOf(tick.bands, node()), dt));
    }),
  );
  return gains;
}
