// Time in ticks: bar:beat:tick, seconds, grid snapping and grid choices.
import { BAR_TICKS, PPQ, STEP_TICKS, type TimeSignature } from "../api/types";
import { t } from "../i18n";

/** "Off" grid: a one-tick step. */
export const FREE_GRID = 1;

/**
 * The "Bar" grid. Bars differ in length (tempo map), so this is a marker, not a step: the timeline snaps to the
 * map's bar boundaries (`domain/tempoMap.ts: snapNearestInBars`), the piano roll to bars of the signature at the clip
 * start. The value is a 4/4 bar so a place that doesn't know the map still gets a sensible step.
 */
export const BAR_GRID = BAR_TICKS;

/**
 * Snap grid choices (timeline, piano roll); except "Bar" they are note lengths counted from the bar start.
 * `label` is a getter so it reads the current language each time it is shown.
 */
export const GRID_CHOICES: readonly { readonly label: string; ticks: number }[] = [
  { get label() { return t().tracks.grid.bar; }, ticks: BAR_GRID },
  { label: "1/2", ticks: PPQ * 2 },
  { get label() { return t().tracks.grid.beat; }, ticks: PPQ },
  { label: "1/8", ticks: PPQ / 2 },
  { label: "1/16", ticks: STEP_TICKS },
  { get label() { return t().tracks.grid.off; }, ticks: FREE_GRID },
];

/** Bar:beat:tick (one-based); computed by `domain/tempoMap.ts: barPosition`. */
export interface BarPosition {
  bar: number;
  beat: number;
  tick: number;
}

export function formatBarPosition({ bar, beat, tick }: BarPosition): string {
  const pad = (value: number, width: number) => String(value).padStart(width, "0");
  return `${pad(bar, 3)}:${pad(beat, 2)}:${pad(tick, 3)}`;
}

export function formatSeconds(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const rest = seconds - minutes * 60;
  return `${minutes}:${rest.toFixed(2).padStart(5, "0")}`;
}

/** Recording duration in whole seconds: "0:07". */
export function formatDuration(seconds: number): string {
  const whole = Math.floor(Math.max(0, seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

export function snapDown(tick: number, grid: number): number {
  return Math.floor(tick / grid) * grid;
}

export function snapNearest(tick: number, grid: number): number {
  return Math.round(tick / grid) * grid;
}

/** Tempo and time signature limits (same as the backend transport). */
export const MIN_TEMPO = 20;
export const MAX_TEMPO = 999;
export const MAX_NUMERATOR = 32;
export const SIGNATURE_DENOMINATORS: readonly number[] = [2, 4, 8, 16];

/** Note recording quantize: the backend's "off" is 0; then grids from a beat down to a sixteenth. */
export const NO_QUANTIZE = 0;
export const RECORD_SNAPS: readonly { readonly label: string; ticks: number }[] = [
  { get label() { return t().tracks.grid.off; }, ticks: NO_QUANTIZE },
  ...GRID_CHOICES.filter((c) => c.ticks !== FREE_GRID && c.ticks <= PPQ),
];

export function barTicks(signature: TimeSignature): number {
  return ((PPQ * 4) / signature.denominator) * signature.numerator;
}
