import { RULER_HEIGHT } from "./timelineLayout";

export const CUE_HALF_WIDTH = 5;
export const CUE_HEIGHT = 9;
export const CUE_TOP = RULER_HEIGHT - CUE_HEIGHT;
/** Extra pixels around the triangle that still count as a hit: it is small. */
const HIT_SLACK = 2;

/** Whether ruler point (x, y) is on the flag of the cue at `cue` ticks. */
export function onCueFlag(cue: number | null, x: number, y: number, pxPerTick: number): boolean {
  if (cue === null || y < CUE_TOP - HIT_SLACK || y > RULER_HEIGHT) return false;
  return Math.abs(x - cue * pxPerTick) <= CUE_HALF_WIDTH + HIT_SLACK;
}
