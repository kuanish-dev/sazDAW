import type { ParamInfo } from "../api/types";
import { clamp } from "./math";

/** A range with a wide spread (hertz, seconds) turns on a logarithmic scale. */
export function isLogScale(info: Pick<ParamInfo, "min" | "max">): boolean {
  return info.min > 0 && info.max / info.min >= 50;
}

export function valueToPosition(value: number, info: Pick<ParamInfo, "min" | "max">): number {
  const clamped = clamp(value, info.min, info.max);
  const position = isLogScale(info)
    ? Math.log(clamped / info.min) / Math.log(info.max / info.min)
    : (clamped - info.min) / (info.max - info.min);
  return clamp(position, 0, 1);
}

export function positionToValue(position: number, info: Pick<ParamInfo, "min" | "max">): number {
  const p = clamp(position, 0, 1);
  return isLogScale(info) ? info.min * (info.max / info.min) ** p : info.min + p * (info.max - info.min);
}

/** Knob angle: -135° (minimum) … +135° (maximum), 0° is up. */
export const KNOB_SWEEP = 270;
export const KNOB_START = -135;
export const KNOB_END = KNOB_START + KNOB_SWEEP;

export function positionToAngle(position: number): number {
  return KNOB_START + KNOB_SWEEP * clamp(position, 0, 1);
}

function polar(cx: number, cy: number, radius: number, angle: number) {
  const rad = (angle * Math.PI) / 180;
  return { x: cx + radius * Math.sin(rad), y: cy - radius * Math.cos(rad) };
}

/** Clockwise arc from angle `from` to `to` (degrees from 12 o'clock). */
export function arcPath(cx: number, cy: number, radius: number, from: number, to: number): string {
  if (Math.abs(to - from) < 0.01) return "";
  const start = polar(cx, cy, radius, from);
  const end = polar(cx, cy, radius, to);
  const large = Math.abs(to - from) > 180 ? 1 : 0;
  const sweep = to > from ? 1 : 0;
  return `M${start.x.toFixed(2)} ${start.y.toFixed(2)} A${radius} ${radius} 0 ${large} ${sweep} ${end.x.toFixed(2)} ${end.y.toFixed(2)}`;
}

/** A parameter with zero inside its range (pan, gain in dB) is drawn as an arc from the center. */
export function isBipolar(info: Pick<ParamInfo, "min" | "max">): boolean {
  return info.min < 0 && info.max > 0;
}

/** Drag distance in pixels that covers the full range. */
const DRAG_RANGE_PX = 200;
/** How many times finer the control is with Shift (drag, wheel, arrows). */
export const FINE_FACTOR = 5;
/** Wheel and arrow step as a fraction of the travel. */
export const WHEEL_STEP = 0.02;
export const KEY_STEP = 0.025;

/** Position change per pixel of drag: `DRAG_RANGE_PX` covers the full range, finer with Shift. */
export function dragDelta(pixels: number, fine: boolean): number {
  return (pixels / DRAG_RANGE_PX) / (fine ? FINE_FACTOR : 1);
}

/** How close (fraction of travel) the knob must be to snap to a snap point. */
export const SNAP_ZONE = 0.03;

/** Parameter snap points as positions: the default value, and zero if it is inside the range. */
export function snapPositions(info: Pick<ParamInfo, "min" | "max" | "default">): number[] {
  const values = isBipolar(info) ? [info.default, 0] : [info.default];
  return [...new Set(values.map((v) => valueToPosition(v, info)))];
}

/** The nearest snap point if the position is within its zone; otherwise the position unchanged. */
export function snapPosition(position: number, points: number[]): number {
  let best = position;
  let distance = SNAP_ZONE;
  for (const point of points) {
    const d = Math.abs(position - point);
    if (d <= distance) { best = point; distance = d; }
  }
  return best;
}
