// The dynamic EQ's graph (`ui/effects/DynamicEqGraph.tsx`): its response with every band at its full gain (as far as it
// can go) and with the bands where they are now (the engine's band gains), the band handles and what dragging them
// changes. MIRRORS `crates/daw-effects/src/dynamic_eq.rs`: four peaking bands (the biquad mirror, `biquad.ts`) in a row,
// each at its gain times how far it is engaged: a change there must be repeated here. The gain axis is the EQ's (±18 dB).
import type { ParamInfo, Params } from "../api/types";
import { magnitudeDb, peaking } from "./biquad";
import type { ParamUpdate } from "./catalog";
import { namedParams } from "./catalog";
import { EQ_RANGE_DB, gridY } from "./eqCurve";
import { fractionToFreq, freqX } from "./frequencyAxis";
import { clamp } from "./math";
import { type PlotView, type Point, linePath } from "./plot";

export const DYNAMIC_EQ_BANDS = 4;
const BAND_PARAMS = ["freq", "q", "gain"] as const;

export interface DynamicBand {
  freq: number;
  q: number;
  /** The most the band turns, dB: negative cuts, positive lifts. */
  gain: number;
  infos: { freq: ParamInfo; q: ParamInfo; gain: ParamInfo };
}

/** The bands by the dynamic EQ's param names; `null` when one is missing. */
export function dynamicEqSettings(infos: ParamInfo[], values: Params): DynamicBand[] | null {
  const bands: DynamicBand[] = [];
  for (let band = 1; band <= DYNAMIC_EQ_BANDS; band++) {
    const name = (what: string) => `band${band}_${what}`;
    const b = namedParams(infos, values, BAND_PARAMS.map(name));
    const [freq, q, gain] = BAND_PARAMS.map((what) => infos.find((p) => p.name === name(what)));
    if (!b || !freq || !q || !gain) return null;
    bands.push({ freq: b[name("freq")]!, q: b[name("q")]!, gain: b[name("gain")]!, infos: { freq, q, gain } });
  }
  return bands;
}

/** The SVG path of the response with band b at `gainsDb[b]` (0 dB where missing). */
export function dynamicEqPath(bands: readonly DynamicBand[], gainsDb: readonly number[], view: PlotView, points = 160): string {
  const filters = bands.map((b, i) => peaking(b.freq, gainsDb[i] ?? 0, b.q));
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const freq = fractionToFreq(i / points);
    line.push([freqX(view, freq), gridY(view, filters.reduce((sum, c) => sum + magnitudeDb(c, freq), 0))]);
  }
  return linePath(line);
}

/** Every band at its full gain: how far the EQ can go. */
export const fullGains = (bands: readonly DynamicBand[]) => bands.map((b) => b.gain);

/** A band's handle: at its frequency and full gain. */
export const bandHandle = (band: DynamicBand, view: PlotView): Point => [freqX(view, band.freq), gridY(view, band.gain)];

/** What dragging a band's handle to point (px, py) of the view sets: its frequency and gain, within their ranges. */
export function dragBand(band: DynamicBand, px: number, py: number, view: PlotView): ParamUpdate[] {
  const { freq, gain } = band.infos;
  const db = EQ_RANGE_DB - (clamp(py, 0, view.height) / view.height) * 2 * EQ_RANGE_DB;
  return [
    { name: freq.name, value: clamp(fractionToFreq(px / view.width), freq.min, freq.max) },
    { name: gain.name, value: clamp(db, gain.min, gain.max) },
  ];
}

/** Wheel over a handle: Q grows (narrower) on wheel up, by a factor per notch, within its range. */
export function wheelBandQ(band: DynamicBand, deltaY: number): ParamUpdate {
  const info = band.infos.q;
  return { name: info.name, value: clamp(band.q * Math.exp(-deltaY * 0.002), info.min, info.max) };
}
