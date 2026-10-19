// The automation curve formula and lane shapes — mirrors of the backend (`daw-contracts` `curve_shape`/`value_at`,
// `daw-project` `automation_shapes.rs`), so the lane is drawn exactly as the engine plays it and the mock draws the
// same shapes.
import { BAR_TICKS, type AutoPoint, type AutomationClip, type AutomationLane, type AutomationShape, type ShapeRequest } from "../api/types";
import { clamp } from "../common/math";

/** How strongly `curve = ±1` bends a segment: the power curve exponent is `CURVE_STRENGTH^|curve|`. */
export const CURVE_STRENGTH = 4;
/** How many points one automation clip can have (`daw_contracts::MAX_POINTS`). */
export const MAX_POINTS = 4096;
/** How many clips one lane can have (`daw_contracts::MAX_AUTOMATION_CLIPS`). */
export const MAX_AUTOMATION_CLIPS = 1024;
/** The longest pattern of an automation clip (`daw_contracts::MAX_CLIP_LOOP_TICKS`): 64 bars of 4/4. */
export const MAX_CLIP_LOOP_TICKS = BAR_TICKS * 64;
/** The shortest period of a repeating shape. */
export const MIN_SHAPE_PERIOD = 2;
/** A sine is drawn with this many straight segments per period. */
const SINE_STEPS = 16;

/** Bent progress at linear progress `t` (0…1): `curve > 0` starts slowly (`t^e`), `curve < 0` starts fast. */
export function curveShape(t: number, curve = 0): number {
  const x = clamp(t, 0, 1);
  const c = Number.isFinite(curve) ? clamp(curve, -1, 1) : 0;
  if (c === 0) return x;
  const exponent = CURVE_STRENGTH ** Math.abs(c);
  return c > 0 ? x ** exponent : 1 - (1 - x) ** exponent;
}

/** Lane value at `tick` (points sorted): holds before the first and after the last point; `null` without points. */
export function valueAt(points: AutoPoint[], tick: number): number | null {
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return null;
  if (tick <= first.tick) return first.value;
  if (tick >= last.tick) return last.value;
  const after = points.findIndex((p) => p.tick > tick);
  const a = points[after - 1]!;
  const b = points[after]!;
  return a.value + (b.value - a.value) * curveShape((tick - a.tick) / (b.tick - a.tick), a.curve);
}

/** Clip value at song `tick`: the pattern repeats every `loopTicks` from the clip start; `null` outside the clip or
 * without points. At a repeat seam the pattern runs to its end (toward a point at `loopTicks`, if any) and the next
 * repeat starts again from its first point. */
export function clipValueAt(clip: AutomationClip, tick: number): number | null {
  if (tick < clip.startTick || tick >= clip.startTick + clip.lengthTicks || clip.loopTicks <= 0) return null;
  return valueAt(clip.points, (tick - clip.startTick) % clip.loopTicks);
}

/** Lane value at song `tick`: the clip under it decides; between clips the lane does not touch the parameter (`null`). */
export function laneValueAt(lane: AutomationLane, tick: number): number | null {
  const clip = lane.clips.find((c) => tick >= c.startTick && tick < c.startTick + c.lengthTicks);
  return clip ? clipValueAt(clip, tick) : null;
}

/** The curve that makes segment `from → to` pass through `middle` halfway (inverse of `curveShape` at 0.5). */
export function curveThrough(from: number, to: number, middle: number): number {
  if (from === to) return 0;
  const ratio = clamp((middle - from) / (to - from), 0.02, 0.98);
  const log4 = (x: number) => Math.log(x) / Math.log(CURVE_STRENGTH);
  const curve = ratio <= 0.5 ? log4(Math.log(ratio) / Math.log(0.5)) : -log4(Math.log(1 - ratio) / Math.log(0.5));
  return clamp(curve, -1, 1);
}

function level(shape: AutomationShape, phase: number): number {
  switch (shape) {
    case "sine":
      return 0.5 + 0.5 * Math.sin(2 * Math.PI * phase);
    case "triangle":
      return 1 - Math.abs(2 * phase - 1);
    case "saw":
    case "rampUp":
      return phase;
    case "square":
      return phase < 0.5 ? 1 : 0;
    case "rampDown":
      return 1 - phase;
  }
}

/** Level at the very end of a period (just before the next starts). */
const endLevel = (shape: AutomationShape): number => (shape === "saw" || shape === "rampUp" ? 1 : shape === "sine" ? 0.5 : 0);

function periodOffsets(shape: AutomationShape, period: number): number[] {
  const half = Math.floor(period / 2);
  switch (shape) {
    case "sine":
      return Array.from({ length: SINE_STEPS }, (_, i) => Math.floor((period * i) / SINE_STEPS));
    case "triangle":
      return [0, half];
    case "saw":
      return [0, period - 1];
    case "square":
      return [0, half - 1, half, period - 1];
    default:
      return [0];
  }
}

/** How many points a shape would produce (to refuse a too dense one before building it). */
export function shapePointCount(r: ShapeRequest): number {
  if (r.shape === "rampUp" || r.shape === "rampDown") return 2;
  return Math.ceil((r.toTick - r.fromTick) / r.periodTicks) * periodOffsets(r.shape, r.periodTicks).length;
}

/** Points of a valid shape request, ascending, one per tick (validation is the caller's job). */
export function shapePoints(r: ShapeRequest): AutoPoint[] {
  const { fromTick: from, toTick: to, shape, periodTicks: period, min, max } = r;
  const value = (fraction: number) => min + (max - min) * fraction;
  if (shape === "rampUp" || shape === "rampDown") return [{ tick: from, value: value(level(shape, 0)) }, { tick: to, value: value(level(shape, 1)) }];
  const phaseAt = (tick: number) => ((tick - from) % period) / period;
  const offsets = periodOffsets(shape, period);
  const points: AutoPoint[] = [];
  for (let start = from; start < to; start += period) {
    for (const offset of offsets) {
      const tick = start + offset;
      if (tick >= to) break;
      const jumpEnd = offset + 1 === period && (shape === "saw" || shape === "square");
      const last = points[points.length - 1];
      if (!last || last.tick < tick) points.push({ tick, value: value(jumpEnd ? endLevel(shape) : level(shape, phaseAt(tick))) });
    }
  }
  const end = (to - from) % period === 0 ? endLevel(shape) : level(shape, phaseAt(to));
  points.push({ tick: to, value: value(end) });
  return points;
}

/** `lane` with the points inside `from…to` replaced by `shape`. */
export const replaceRange = (lane: AutoPoint[], from: number, to: number, shape: AutoPoint[]): AutoPoint[] =>
  [...lane.filter((p) => p.tick < from || p.tick > to), ...shape].sort((a, b) => a.tick - b.tick);
