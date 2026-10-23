import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import { clamp } from "../common/math";
import type { PlotView, Point } from "../common/plot";
import { linePath } from "../common/plot";

/** The graph's delay axis, 0 ms (bottom) to the length of the chorus's lines (top). */
export const CHORUS_MAX_MS = 50;
/** How much time the graph shows, seconds: the newest at the right edge. Fixed, so a faster rate shows more waves. */
const SWEEP_SECONDS = 2;

export interface ChorusSettings {
  rate: number;
  depth: number;
  delay: number;
  mix: number;
}

export const chorusSettings = (infos: ParamInfo[], values: Params): ChorusSettings | null =>
  namedParams(infos, values, ["rate", "depth", "delay", "mix"]);

/** The voices: 0 the left, 1 the right (a quarter turn ahead). */
export type Voice = 0 | 1;

/** A voice's delay in ms at LFO `phase` (radians). */
export const voiceDelayMs = (s: ChorusSettings, phase: number, voice: Voice) => s.delay + s.depth * Math.sin(phase + (voice * Math.PI) / 2);

/** The LFO phase `dt` seconds later. */
export const advancePhase = (phase: number, rate: number, dt: number) => (phase + 2 * Math.PI * rate * dt) % (2 * Math.PI);

/** The y of a delay in the view. */
export const delayY = (view: PlotView, ms: number) => view.height * (1 - clamp(ms, 0, CHORUS_MAX_MS) / CHORUS_MAX_MS);

/** A voice's delay over the last `SWEEP_SECONDS`, `phase` being the LFO now (at the right edge). */
export function sweepPath(s: ChorusSettings, phase: number, voice: Voice, view: PlotView, points = 160): string {
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const ago = SWEEP_SECONDS * (1 - i / points);
    line.push([(i / points) * view.width, delayY(view, voiceDelayMs(s, phase - 2 * Math.PI * s.rate * ago, voice))]);
  }
  return linePath(line);
}
