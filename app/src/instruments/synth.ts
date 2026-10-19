// The built-in synth's picture in the instrument window: one cycle of its waveform and its filter response.
// The math mirrors the synth itself (`crates/daw-synth/src/osc.rs`, `filter.rs`, `engine.rs`): the same five waveforms,
// a one-pole highpass (low cut) and a resonant 12 dB/octave lowpass (high cut).
import { clamp } from "../common/math";

/** Kind of the built-in synth in the instrument catalog. */
export const SYNTH_KIND = "synth";

/** As `MAX_RESONANCE` in `filter.rs`: how far full resonance lowers the lowpass damping from 2. */
const MAX_RESONANCE = 1.9;

/** A fixed jagged line for the noise option: the same picture every time. */
function noiseAt(t: number): number {
  const x = Math.sin(t * 12_989.8) * 43_758.5453;
  return 2 * (x - Math.floor(x)) - 1;
}

/** Value of waveform option `index` (Sine, Triangle, Saw, Square, Noise) at phase `t` (`0..1`); the result is in
 *  `[-1, 1]`. */
export function oscillator(index: number, t: number): number {
  switch (Math.round(index)) {
    case 0:
      return Math.sin(2 * Math.PI * t);
    case 1:
      return 4 * Math.abs(t - 0.5) - 1;
    case 2:
      return 2 * t - 1;
    case 3:
      return t < 0.5 ? 1 : -1;
    default:
      return noiseAt(t);
  }
}

/** Response in dB at `hz` of the one-pole highpass at `lowCutHz` (−3 dB at its corner) followed by the 12 dB/octave
 *  lowpass at `highCutHz` with `resonancePercent` (`0..100`: −6 dB at the corner without resonance, a peak with it). */
export function filterDb(hz: number, lowCutHz: number, highCutHz: number, resonancePercent = 0): number {
  const x = hz / highCutHz;
  const damping = 2 - MAX_RESONANCE * clamp(resonancePercent / 100, 0, 1);
  const lowpass = -10 * Math.log10((1 - x * x) ** 2 + (damping * x) ** 2);
  return lowpass - 10 * Math.log10(1 + (lowCutHz / hz) ** 2);
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

/** The headroom above 0 dB for a resonance peak. */
const CEILING_DB = 12;

/** The filter response from 20 Hz to 20 kHz, `CEILING_DB` at the top, `FLOOR_DB` at the bottom. */
export function filterPath(lowCutHz: number, highCutHz: number, resonancePercent: number, width: number, height: number, pad: number): string {
  const points: [number, number][] = [];
  for (let i = 0; i <= STEPS; i++) {
    const x = i / STEPS;
    const hz = MIN_HZ * (MAX_HZ / MIN_HZ) ** x;
    const db = filterDb(hz, lowCutHz, highCutHz, resonancePercent);
    points.push([x, clamp((db - FLOOR_DB) / (CEILING_DB - FLOOR_DB), 0, 1)]);
  }
  return unitPath(points, width, height, pad);
}

/** Horizontal position of a cut marker in the same box as `filterPath`. */
export function cutoffX(cutoffHz: number, width: number, pad: number): number {
  return pad + clamp(hzToX(cutoffHz), 0, 1) * (width - pad * 2);
}
