// The transient shaper's graph (`ui/effects/TransientGraph.tsx`): what its settings do to a model hit, and the live
// sound as envelopes going in and coming out. The model MIRRORS `crates/daw-effects/src/transient.rs`: the same
// followers and times run on the envelope of a hit (silence, an instant rise, an exponential fall), the gain
// `attack·transient + sustain·tail` capped at ±18 dB and smoothed: a change there must be repeated here. The envelope
// stands for the peak of the sound; the effect's mean square of a sine reads √2 lower, which changes no ratio.
import { type PlotView, type Point, dbY, linePath, toDb } from "./plot";

const PEAK_RELEASE_MS = 250;
const TRANSIENT_MS = 20;
const LEVEL_MS = 10;
const HOLD_RELEASE_MS = 300;
const MAX_SHAPE_DB = 18;
const GAIN_MS = 1;
const FLOOR = 1e-5;

/** The model runs at this rate: fine enough for the 1-ms gain smoothing, cheap enough to rerun on every knob move. */
const MODEL_RATE = 8_000;
/** The model hit: this much silence, then a level of 1 dying away with `HIT_DECAY_MS`, over `MODEL_MS` in all. */
const LEAD_MS = 20;
const HIT_DECAY_MS = 120;
export const MODEL_MS = 400;
/** The model's level axis: the hit starts at 0 dB, a full boost lifts it 18 dB above. */
export const MODEL_TOP_DB = 18;
export const MODEL_FLOOR_DB = -48;
/** Every how many model steps a point is drawn. */
const DRAW_EVERY = 8;

/** One-pole coefficient of a time constant at the model rate, as `time_coefficient` (0 ms instant, as `Follower`). */
const coefficient = (ms: number) => (ms > 0 ? Math.exp(-1 / Math.max(1, ms * 0.001 * MODEL_RATE)) : 0);

/** A follower (`dynamics.rs: Follower`): rises with the attack, falls with the release. */
function follower(attackMs: number, releaseMs: number): (target: number) => number {
  const [attack, release] = [coefficient(attackMs), coefficient(releaseMs)];
  let value = 0;
  return (target) => {
    value = target + (target > value ? attack : release) * (value - target);
    return value;
  };
}

export interface ShapedPoint {
  ms: number;
  /** The hit's level and the shaped level, dB. */
  input: number;
  output: number;
}

/** The model hit before and after the shaper; `attack` and `sustain` are −1..1 (the knobs' % / 100). */
export function shapeModelHit(attack: number, sustain: number): ShapedPoint[] {
  const peak = follower(0, PEAK_RELEASE_MS);
  const slowPeak = follower(TRANSIENT_MS, 0);
  const hold = follower(0, HOLD_RELEASE_MS);
  const gain = follower(GAIN_MS, GAIN_MS);
  const levelCoefficient = coefficient(LEVEL_MS);
  let meanSquare = 0;
  const points: ShapedPoint[] = [];
  const steps = (MODEL_MS * MODEL_RATE) / 1000;
  for (let i = 0; i < steps; i++) {
    const ms = (i * 1000) / MODEL_RATE;
    const x = ms < LEAD_MS ? 0 : Math.exp(-(ms - LEAD_MS) / HIT_DECAY_MS);
    const fast = Math.max(FLOOR, peak(x));
    const slow = Math.max(FLOOR, slowPeak(fast));
    meanSquare = x * x + levelCoefficient * (meanSquare - x * x);
    const level = Math.max(FLOOR, Math.sqrt(meanSquare));
    const held = Math.max(FLOOR, hold(level));
    const shape = attack * 20 * Math.log10(fast / slow) + sustain * 20 * Math.log10(held / level);
    const db = gain(Math.max(-MAX_SHAPE_DB, Math.min(MAX_SHAPE_DB, shape)));
    if (i % DRAW_EVERY === 0) points.push({ ms, input: toDb(x), output: toDb(x) + db });
  }
  return points;
}

/** The model's paths in a view: time across, level up (below the floor: a gap). */
export function modelPaths(points: readonly ShapedPoint[], view: PlotView): { input: string; output: string } {
  const at = (db: number, ms: number): Point | null => (db > MODEL_FLOOR_DB ? [(ms / MODEL_MS) * view.width, dbY(view, db, MODEL_FLOOR_DB, MODEL_TOP_DB)] : null);
  return { input: linePath(points.map((p) => at(p.input, p.ms))), output: linePath(points.map((p) => at(p.output, p.ms))) };
}

/** How long one segment of the live envelopes is, seconds: fine enough to show a hit's start. */
export const SEGMENT_SECONDS = 0.005;

/** The peak of each `SEGMENT_SECONDS` of the newest `frames` frames of `samples`, oldest first (dBFS). */
export function envelopeSegments(samples: Float32Array, sampleRate: number, frames: number): number[] {
  const size = Math.max(1, Math.round(sampleRate * SEGMENT_SECONDS));
  const count = Math.floor(Math.min(frames, samples.length) / size);
  const from = samples.length - count * size;
  return Array.from({ length: count }, (_, s) => {
    let peak = 0;
    for (let i = from + s * size; i < from + (s + 1) * size; i++) peak = Math.max(peak, Math.abs(samples[i]!));
    return toDb(peak);
  });
}
