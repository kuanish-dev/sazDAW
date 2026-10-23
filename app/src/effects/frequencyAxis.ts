import { clamp } from "../common/math";
import type { PlotView } from "../common/plot";

export const FREQ_MIN_HZ = 20;
export const FREQ_MAX_HZ = 20_000;

/** Frequency ↔ position along the axis (0..1), logarithmic. */
export const freqToFraction = (freq: number) => Math.log(freq / FREQ_MIN_HZ) / Math.log(FREQ_MAX_HZ / FREQ_MIN_HZ);
export const fractionToFreq = (fraction: number) => FREQ_MIN_HZ * (FREQ_MAX_HZ / FREQ_MIN_HZ) ** clamp(fraction, 0, 1);

/** The x of a frequency in a view. */
export const freqX = (view: PlotView, freq: number) => freqToFraction(freq) * view.width;

/** Frequencies of the vertical grid lines. */
export const FREQ_GRID_HZ = [50, 100, 200, 500, 1_000, 2_000, 5_000, 10_000] as const;

/** "1k", "200" — short frequency labels for the axis. */
export const formatHz = (freq: number) => (freq >= 1_000 ? `${Number((freq / 1_000).toFixed(1))}k` : `${Math.round(freq)}`);
