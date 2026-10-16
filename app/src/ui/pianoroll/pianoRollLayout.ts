// Piano roll sizes and coordinate mapping: pitch ↔ row, tick ↔ x, velocity ↔ bar height, controller value fraction ↔ y.
// Pure functions, no DOM.
import { clamp } from "../../domain/math";
import { HIGH_PITCH, LOW_PITCH, MAX_VELOCITY, clampPitch } from "../../domain/pitch";

export const KEYS_WIDTH = 64;
export const ROW_HEIGHT = 16;
/** Width of the zone at a note's right edge used to drag its length. */
export const HANDLE_PX = 8;
/** Minimum note length without grid snapping. */
export const FREE_MIN_TICKS = 30;
export const ROWS = HIGH_PITCH - LOW_PITCH + 1;
export const GRID_HEIGHT = ROWS * ROW_HEIGHT;
/** Pixels per tick at 100% zoom. */
export const BASE_PX_PER_TICK = 0.2;
export const ZOOMS = [0.25, 0.5, 1, 2, 4];

export const rowY = (pitch: number) => (HIGH_PITCH - pitch) * ROW_HEIGHT;
export const pitchAtY = (y: number) => clampPitch(HIGH_PITCH - Math.floor(y / ROW_HEIGHT));
export const tickX = (tick: number, pxPerTick: number) => KEYS_WIDTH + tick * pxPerTick;
export const tickAtX = (x: number, pxPerTick: number) => Math.max(0, (x - KEYS_WIDTH) / pxPerTick);

// Velocity lane under the grid: a note's bar grows upward from the bottom line.
export const VELOCITY_HEIGHT = 72;
/** Bottom line of the bars, and bar height at velocity 127. */
const VELOCITY_BASE = VELOCITY_HEIGHT - 2;
const VELOCITY_SPAN = VELOCITY_HEIGHT - 8;

/** Velocity bar height (the bar top is `VELOCITY_BASE - height`). */
export const velocityBarHeight = (velocity: number) => (velocity / MAX_VELOCITY) * VELOCITY_SPAN;
export const velocityBarTop = (velocity: number) => VELOCITY_BASE - velocityBarHeight(velocity);

/** Velocity whose bar top lands at height `y` (1…127). */
export const velocityAtY = (y: number) => clamp(Math.round(((VELOCITY_BASE - y) / VELOCITY_SPAN) * MAX_VELOCITY), 1, MAX_VELOCITY);

// Controller lane under velocity: value fraction 0…1 (bottom … top) with top and bottom padding.
export const CONTROLLER_HEIGHT = 72;
const CONTROLLER_PAD = 6;
const CONTROLLER_SPAN = CONTROLLER_HEIGHT - 2 * CONTROLLER_PAD;

export const controllerY = (fraction: number) => CONTROLLER_PAD + (1 - fraction) * CONTROLLER_SPAN;
export const fractionAtY = (y: number) => clamp((CONTROLLER_PAD + CONTROLLER_SPAN - y) / CONTROLLER_SPAN, 0, 1);

/** Wheel zoom step: the next or previous value from `ZOOMS`. */
export function nextZoom(zoom: number, up: boolean): number {
  const index = ZOOMS.indexOf(zoom);
  return ZOOMS[clamp(index + (up ? 1 : -1), 0, ZOOMS.length - 1)] ?? zoom;
}
