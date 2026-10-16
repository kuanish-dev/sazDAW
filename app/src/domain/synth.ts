// The built-in synth's picture in the instrument window: one cycle of its waveform and its filter response.
// The math mirrors the synth itself (`crates/daw-synth/src/engine.rs`): the same four waveforms, a one-pole highpass
// (low cut) and a one-pole lowpass (high cut).
import { clamp } from "./math";

/** Kind of the built-in synth in the instrument catalog. */
export const SYNTH_KIND = "synth";

/** Value of waveform option `index` (Sine, Triangle, Saw, Square) at phase `t` (`0..1`); the result is in `[-1, 1]`. */
export function oscillator(index: number, t: number): number {
  switch (Math.round(index)) {
    case 0:
      return Math.sin(2 * Math.PI * t);
    case 1:
      return 4 * Math.abs(t - 0.5) - 1;
    case 2:
      return 2 * t - 1;
    default:
      return t < 0.5 ? 1 : -1;
  }
}

/** Response in dB at `hz` of the one-pole highpass at `lowCutHz` followed by the one-pole lowpass at `highCutHz`
 *  (each −3 dB at its corner). */
export function filterDb(hz: number, lowCutHz: number, highCutHz: number): number {
  return -10 * Math.log10(1 + (hz / highCutHz) ** 2) - 10 * Math.log10(1 + (lowCutHz / hz) ** 2);
}

const MIN_HZ = 20;
const MAX_HZ = 20_000;
/** The bottom of the response graph. */
const FLOOR_DB = -36;

/** Position of a frequency on a logarithmic `0..1` axis from 20 Hz to 20 kHz. */
export function hzToX(hz: number): number {
  return Math.log10(hz / MIN_HZ) / Math.log10(MAX_HZ / MIN_HZ);
}

/** An SVG path through points given in the unit square (x right, y up) inside a `width`×`height` box with padding. */
function unitPath(points: [number, number][], width: number, height: number, pad: number): string {
  const sx = (x: number) => pad + x * (width - pad * 2);
  const sy = (y: number) => height - pad - y * (height - pad * 2);
  return points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${sx(x).toFixed(1)} ${sy(y).toFixed(1)}`).join(" ");
}

const STEPS = 96;

/** One cycle of waveform `index`, centred vertically. */
export function wavePath(index: number, width: number, height: number, pad: number): string {
  const points: [number, number][] = [];
  for (let i = 0; i <= STEPS; i++) {
    const t = i / STEPS;
    // The square's jump is drawn as a vertical edge, not a slope between two samples.
    if (Math.round(index) === 3 && i === STEPS / 2) points.push([t, 0.5 + 0.45 * oscillator(index, t - 1e-6)]);
    points.push([t, 0.5 + 0.45 * oscillator(index, Math.min(t, 1 - 1e-6))]);
  }
  return unitPath(points, width, height, pad);
}

/** The filter response from 20 Hz to 20 kHz, 0 dB at the top, `FLOOR_DB` at the bottom. */
export function filterPath(lowCutHz: number, highCutHz: number, width: number, height: number, pad: number): string {
  const points: [number, number][] = [];
  for (let i = 0; i <= STEPS; i++) {
    const x = i / STEPS;
    const hz = MIN_HZ * (MAX_HZ / MIN_HZ) ** x;
    points.push([x, clamp(1 - filterDb(hz, lowCutHz, highCutHz) / FLOOR_DB, 0, 1)]);
  }
  return unitPath(points, width, height, pad);
}

/** Horizontal position of a cut marker in the same box as `filterPath`. */
export function cutoffX(cutoffHz: number, width: number, pad: number): number {
  return pad + clamp(hzToX(cutoffHz), 0, 1) * (width - pad * 2);
}
