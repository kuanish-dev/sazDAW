// The live spectrum behind the EQ and filter curves (`effects/LiveSpectrum.tsx`, FabFilter Pro-Q style): a window of
// the node's output → levels at points spread evenly over the graph's log-frequency axis → smoothed over time → a filled
// SVG area in the graph's view. The FFT is the master visualizer's (`visualizer.ts: magnitudes`), the axis is the graphs'
// own (`frequencyAxis.ts`), so the spectrum lines up with the curve and the frequency grid.
import { fractionToFreq } from "./frequencyAxis";
import { clamp } from "../common/math";
import type { PlotView } from "../common/plot";
import { magnitudes } from "../mixer/visualizer";

/** Points across the graph: one per 2 px of its 300-px view, enough for a smooth outline. */
export const SPECTRUM_POINTS = 150;
/** The spectrum's level scale, dBFS: the top of the graph and its bottom (quieter reads as empty). It is not the curve's
 *  gain scale (±18 dB for the EQ): one FFT bin of a mix spans 60–80 dB between the bass and the air, so a 36-dB window
 *  would cut off half of it. 90 dB is Pro-Q's default analyzer range. */
const TOP_DB = 0;
const FLOOR_DB = -90;
/** Tilt, dB per octave around 1 kHz: music loses roughly this much per octave upwards (and a bin holds more hertz of it
 *  higher up), so without the tilt the highs hug the floor. Pro-Q's default. */
const TILT_DB_PER_OCTAVE = 4.5;
const TILT_PIVOT_HZ = 1_000;
/** Time constants, seconds: a level rises almost at once and falls slowly, so the picture is lively but calm, and
 *  silence (playback stopped) fades it out within about a second. */
const RISE_SECONDS = 0.03;
const FALL_SECONDS = 0.25;
/** Below this every point counts as the floor: the picture has faded out and nothing is drawn. */
const QUIET = 0.005;

/** The amplitude around `freq`. Where the point's stretch of the axis (`low`..`high`) holds two bins or more (the highs),
 *  their power mean: an even line instead of single noisy bins. Elsewhere (the lows, where a bin is wider than a point)
 *  the value between the two nearest bins, so the line does not step. */
function amplitudeAt(bins: Float32Array, binHz: number, low: number, high: number, freq: number): number {
  const top = bins.length - 1;
  const first = Math.ceil(low / binHz);
  const last = Math.min(top, Math.floor(high / binHz));
  if (last > first) {
    let power = 0;
    for (let k = first; k <= last; k++) power += bins[k]! ** 2;
    return Math.sqrt(power / (last - first + 1));
  }
  const position = Math.min(top, freq / binHz);
  const below = Math.floor(position);
  const above = Math.min(top, below + 1);
  return bins[below]! + (bins[above]! - bins[below]!) * (position - below);
}

/** Levels 0..1 (the bottom..top of the graph) at `SPECTRUM_POINTS` frequencies spread evenly over the axis; all 0
 *  for no samples. The sample count must be a power of two (`NODE_WINDOW_FRAMES`). */
export function spectrumLevels(samples: Float32Array, sampleRate: number): number[] {
  if (samples.length === 0 || sampleRate <= 0) return Array.from({ length: SPECTRUM_POINTS }, () => 0);
  const bins = magnitudes(samples);
  const binHz = sampleRate / samples.length;
  return Array.from({ length: SPECTRUM_POINTS }, (_, i) => {
    const at = (offset: number) => fractionToFreq((i + offset) / (SPECTRUM_POINTS - 1));
    const freq = at(0);
    const db = 20 * Math.log10(amplitudeAt(bins, binHz, at(-0.5), at(0.5), freq)) + TILT_DB_PER_OCTAVE * Math.log2(freq / TILT_PIVOT_HZ);
    return Number.isFinite(db) ? clamp((db - FLOOR_DB) / (TOP_DB - FLOOR_DB), 0, 1) : 0;
  });
}

/** The shown levels after `dt` seconds of moving toward `fresh` ones: fast up, slow down. */
export function smoothLevels(shown: readonly number[], fresh: readonly number[], dt: number): number[] {
  return fresh.map((level, i) => {
    const previous = shown[i] ?? 0;
    const seconds = level > previous ? RISE_SECONDS : FALL_SECONDS;
    return previous + (level - previous) * (1 - Math.exp(-dt / seconds));
  });
}

/** The levels as a closed SVG area over the bottom of the view; empty when every level is at the floor. */
export function spectrumPath(levels: readonly number[], view: PlotView): string {
  if (levels.every((level) => level < QUIET)) return "";
  const step = view.width / (levels.length - 1);
  const outline = levels.map((level, i) => `L${(i * step).toFixed(1)} ${(view.height * (1 - level)).toFixed(1)}`);
  return `M0 ${view.height} ${outline.join(" ")} L${view.width} ${view.height} Z`;
}
