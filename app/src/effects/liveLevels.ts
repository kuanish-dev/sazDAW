// Levels of a node's live sound over time, for the effect pictures (`effects/*Graph.tsx`): how loud the newest stretch
// of a window is, a short history of such readings that scrolls with time, and a follower that rises fast and falls
// slowly, so a picture calms down and fades out when the sound stops instead of blinking off.
import type { Point } from "../common/plot";

/** How much of the newest sound one reading measures, seconds: about one poll interval, so consecutive readings cover
 *  the sound once (the windows overlap: 85 ms of sound read every 33 ms). */
export const READING_SECONDS = 0.035;

/** The newest `seconds` of `samples` (all of them when shorter). */
function newest(samples: Float32Array, sampleRate: number, seconds: number): Float32Array {
  const count = Math.max(1, Math.round(sampleRate * seconds));
  return samples.length > count ? samples.subarray(samples.length - count) : samples;
}

/** Peak amplitude (0..1+) of the newest `seconds` of `samples`; 0 for none. */
export function recentPeak(samples: Float32Array, sampleRate: number, seconds = READING_SECONDS): number {
  let peak = 0;
  for (const sample of newest(samples, sampleRate, seconds)) peak = Math.max(peak, Math.abs(sample));
  return peak;
}

/** RMS amplitude of the newest `seconds` of `samples` (the energy, for a reverb tail); 0 for none. */
export function recentRms(samples: Float32Array, sampleRate: number, seconds = READING_SECONDS): number {
  const part = newest(samples, sampleRate, seconds);
  if (part.length === 0) return 0;
  let power = 0;
  for (const sample of part) power += sample * sample;
  return Math.sqrt(power / part.length);
}

/** One reading of a history: when it was taken, seconds on the picture's clock, plus its values. */
export interface Reading {
  at: number;
}

/** The history with `reading` added and the readings older than `keepSeconds` before it dropped. */
export function pushReading<R extends Reading>(history: readonly R[], reading: R, keepSeconds: number): R[] {
  const from = reading.at - keepSeconds;
  const kept = history.filter((r) => r.at >= from);
  kept.push(reading);
  return kept;
}

/** The reading at or just before `at`; `undefined` when the history does not reach back that far. */
export function readingAt<R extends Reading>(history: readonly R[], at: number): R | undefined {
  for (let i = history.length - 1; i >= 0; i--) {
    if (history[i]!.at <= at) return history[i];
  }
  return undefined;
}

/** The history as points: x from the reading's time, y from its value; `null` (a gap) where the value is at or below
 *  `floor`, so silence draws nothing. */
export function historyPoints<R extends Reading>(
  history: readonly R[],
  value: (reading: R) => number,
  toX: (at: number) => number,
  toY: (value: number) => number,
  floor: number,
): (Point | null)[] {
  return history.map((r) => {
    const v = value(r);
    return v > floor ? [toX(r.at), toY(v)] : null;
  });
}

/** A shown value after `dt` seconds of moving toward `fresh`: within about `riseSeconds` up, `fallSeconds` down. */
export function follow(shown: number, fresh: number, dt: number, riseSeconds: number, fallSeconds: number): number {
  const seconds = fresh > shown ? riseSeconds : fallSeconds;
  return shown + (fresh - shown) * (1 - Math.exp(-dt / seconds));
}
