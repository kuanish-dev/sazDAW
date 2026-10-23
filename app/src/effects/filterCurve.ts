import type { ParamInfo, Params } from "../api/types";
import { type ParamUpdate, namedParams } from "../common/catalog";
import { FREQ_MAX_HZ, fractionToFreq, freqX } from "./frequencyAxis";
import { clamp } from "../common/math";
import { type PlotView, type Point, dbY, linePath } from "../common/plot";

/** Sample rate for drawing: the response near the top of the axis differs slightly at other rates. */
const DRAW_RATE = 48_000;
/** The gain axis: room for the resonance peak (Q 12 is +21.6 dB) above, and the slope of the cut below. */
const FILTER_TOP_DB = 24;
const FILTER_FLOOR_DB = -36;
export const FILTER_GRID_DB = [12, 0, -12, -24] as const;
/** The `type` choices: low-pass, high-pass, band-pass. */
const BAND_PASS = 2;

export interface FilterSettings {
  type: number;
  cutoff: number;
  resonance: number;
  mix: number;
  infos: { cutoff: ParamInfo; resonance: ParamInfo };
}

/** The settings by the filter's param names; `null` when one is missing. */
export function filterSettings(infos: ParamInfo[], values: Params): FilterSettings | null {
  const s = namedParams(infos, values, ["type", "cutoff", "resonance", "mix"]);
  const cutoff = infos.find((p) => p.name === "cutoff");
  const resonance = infos.find((p) => p.name === "resonance");
  return s && cutoff && resonance ? { ...s, type: Math.trunc(s.type), infos: { cutoff, resonance } } : null;
}

/** Response at `freq`, dB. */
function filterResponseDb(s: FilterSettings, freq: number): number {
  const g = Math.tan((Math.PI * Math.min(s.cutoff, DRAW_RATE * 0.45)) / DRAW_RATE);
  const w = Math.tan((Math.PI * freq) / DRAW_RATE) / g;
  const k = 1 / s.resonance;
  const [denRe, denIm] = [1 - w * w, k * w];
  const [numRe, numIm] = s.type === BAND_PASS ? [0, k * w] : s.type === 1 ? [-w * w, 0] : [1, 0];
  const den = denRe * denRe + denIm * denIm;
  const re = (numRe * denRe + numIm * denIm) / den;
  const im = (numIm * denRe - numRe * denIm) / den;
  const [mixRe, mixIm] = [1 + (re - 1) * s.mix, im * s.mix];
  return 10 * Math.log10(Math.max(mixRe * mixRe + mixIm * mixIm, 1e-20));
}

const y = (view: PlotView, db: number) => dbY(view, db, FILTER_FLOOR_DB, FILTER_TOP_DB);
export const filterGridY = y;

/** The SVG path of the response curve. */
export function filterCurvePath(s: FilterSettings, view: PlotView, points = 160): string {
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const freq = fractionToFreq(i / points);
    line.push([freqX(view, freq), y(view, filterResponseDb(s, freq))]);
  }
  return linePath(line);
}

/** Whether height on the graph sets the resonance: for low- and high-pass the peak at the cutoff is Q itself (in dB),
 *  the band-pass is 0 dB there at any Q. */
const heightIsResonance = (s: FilterSettings) => s.type !== BAND_PASS;

/** The handle at the cutoff: at the height of Q for low- and high-pass (on the curve at full mix, above it when the dry
 *  sound is mixed in, so dragging never jumps), at 0 dB for the band-pass. */
export function filterHandle(s: FilterSettings, view: PlotView): Point {
  return [freqX(view, s.cutoff), y(view, heightIsResonance(s) ? 20 * Math.log10(s.resonance) : 0)];
}

/** What dragging the handle to point (px, py) of the view sets: the cutoff, and the resonance for low- and high-pass. */
export function dragFilterHandle(s: FilterSettings, px: number, py: number, view: PlotView): ParamUpdate[] {
  const { cutoff, resonance } = s.infos;
  const updates = [{ name: cutoff.name, value: clamp(fractionToFreq(px / view.width), cutoff.min, Math.min(cutoff.max, FREQ_MAX_HZ)) }];
  if (heightIsResonance(s)) {
    const db = FILTER_TOP_DB - (clamp(py, 0, view.height) / view.height) * (FILTER_TOP_DB - FILTER_FLOOR_DB);
    updates.push({ name: resonance.name, value: clamp(10 ** (db / 20), resonance.min, resonance.max) });
  }
  return updates;
}

/** Wheel over the handle: resonance grows on wheel up, by a factor per notch, within its range. */
export function wheelResonance(s: FilterSettings, deltaY: number): ParamUpdate {
  const info = s.infos.resonance;
  return { name: info.name, value: clamp(s.resonance * Math.exp(-deltaY * 0.002), info.min, info.max) };
}
