// The multiband compressor's graph (`ui/effects/MultibandGraph.tsx`): where the crossover splits the sound, the shape
// of every band, and what all the bands together do to the sound now. MIRRORS `crates/daw-effects/src/multiband.rs`
// and its Linkwitz–Riley crossover `crossover.rs`: each split at least at the one below it; band b is the low side of
// split b (two Butterworth low-passes) after the high sides of the splits below it and through the all-passes of the
// splits above it, the top band the high sides of all; with three bands the top two play as one with band 3's settings;
// a bypassed band passes at 0 dB, a soloed band silences the others: a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { BUTTERWORTH_Q, type Coefficients, type Complex, allPass, chainResponse, complexDb, pass } from "./biquad";
import type { ParamUpdate } from "./catalog";
import { namedParams } from "./catalog";
import { FREQ_MAX_HZ, FREQ_MIN_HZ, fractionToFreq, freqX } from "./frequencyAxis";
import { clamp } from "./math";
import { type PlotView, type Point, dbY, linePath } from "./plot";

export const MULTIBAND_BANDS = 4;
const SPLIT_PARAMS = ["split1", "split2", "split3"] as const;
const BAND_PARAMS = ["threshold", "ratio", "gain", "solo", "bypass"] as const;

/** The level axis of the graph: room for a band's gain above 0 dB and for deep reduction below. */
const TOP_DB = 12;
const FLOOR_DB = -30;
export const MULTIBAND_GRID_DB = [6, 0, -12, -24];
export const multibandY = (view: PlotView, db: number) => dbY(view, db, FLOOR_DB, TOP_DB);

export interface MultibandBand {
  threshold: number;
  ratio: number;
  gain: number;
  solo: boolean;
  bypass: boolean;
}

export interface MultibandSettings {
  /** The split frequencies as the effect plays them, Hz: each at least at the one below it. */
  splits: number[];
  /** The bands in use: 3 or 4. */
  count: number;
  bands: MultibandBand[];
  /** The split params, for dragging the split lines. */
  splitInfos: ParamInfo[];
}

/** The settings by the multiband compressor's param names; `null` when one is missing. */
export function multibandSettings(infos: ParamInfo[], values: Params): MultibandSettings | null {
  const top = namedParams(infos, values, [...SPLIT_PARAMS, "bands"]);
  const splitInfos = SPLIT_PARAMS.map((name) => infos.find((p) => p.name === name));
  if (!top || splitInfos.some((info) => info === undefined)) return null;
  const bands: MultibandBand[] = [];
  for (let band = 1; band <= MULTIBAND_BANDS; band++) {
    const b = namedParams(infos, values, BAND_PARAMS.map((what) => `band${band}_${what}`));
    if (!b) return null;
    const at = (what: (typeof BAND_PARAMS)[number]) => b[`band${band}_${what}`]!;
    bands.push({ threshold: at("threshold"), ratio: at("ratio"), gain: at("gain"), solo: at("solo") >= 0.5, bypass: at("bypass") >= 0.5 });
  }
  let floor = 0;
  const splits = SPLIT_PARAMS.map((name) => (floor = Math.max(floor, top[name])));
  return { splits, count: top.bands >= 1 ? 4 : 3, bands, splitInfos: splitInfos as ParamInfo[] };
}

/** The filters of one split. */
function splitFilters(freq: number): { low: Coefficients; high: Coefficients; allPass: Coefficients } {
  return { low: pass(freq, BUTTERWORTH_Q, false), high: pass(freq, BUTTERWORTH_Q, true), allPass: allPass(freq, BUTTERWORTH_Q) };
}

/** The filters band `band` of four goes through. */
function bandChain(splits: ReturnType<typeof splitFilters>[], band: number): Coefficients[] {
  const chain: Coefficients[] = [];
  splits.forEach((split, s) => {
    if (s < band) chain.push(split.high, split.high);
    else if (s === band) chain.push(split.low, split.low);
    else chain.push(split.allPass);
  });
  return chain;
}

const plus = ([ar, ai]: Complex, [br, bi]: Complex): Complex => [ar + br, ai + bi];
const scaled = ([re, im]: Complex, k: number): Complex => [re * k, im * k];

/** The complex responses of the bands in use at `freq`, lowest first (with three bands the third holds the fourth). */
function bandResponses(s: MultibandSettings, freq: number): Complex[] {
  const splits = s.splits.map(splitFilters);
  const four = Array.from({ length: MULTIBAND_BANDS }, (_, band) => chainResponse(bandChain(splits, band), freq));
  return s.count === 4 ? four : [four[0]!, four[1]!, plus(four[2]!, four[3]!)];
}

/** Each band's level at `freq`, dB. */
export const bandLevelsDb = (s: MultibandSettings, freq: number): number[] => bandResponses(s, freq).map(complexDb);

/** Each band in use as it plays now, a linear gain: its own gain minus what it takes off now (`liveDb`, 0 dB or less per
 *  band, from the engine), 0 dB when bypassed, silent when another band is soloed. */
export function playedGains(s: MultibandSettings, liveDb: readonly number[]): number[] {
  const bands = s.bands.slice(0, s.count);
  const anySolo = bands.some((b) => b.solo);
  return bands.map((b, i) => {
    if (anySolo && !b.solo) return 0;
    return b.bypass ? 1 : 10 ** ((b.gain + Math.min(0, liveDb[i] ?? 0)) / 20);
  });
}

/** The SVG path of the whole effect's response with the bands at `gains` (linear, `playedGains`). */
export function multibandCurvePath(s: MultibandSettings, gains: readonly number[], view: PlotView, points = 160): string {
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const freq = fractionToFreq(i / points);
    const total = bandResponses(s, freq).reduce<Complex>((sum, band, b) => plus(sum, scaled(band, gains[b] ?? 0)), [0, 0]);
    line.push([freqX(view, freq), multibandY(view, complexDb(total))]);
  }
  return linePath(line);
}

/** The SVG path of one band's shape (at 0 dB gain). */
export function bandShapePath(s: MultibandSettings, band: number, view: PlotView, points = 120): string {
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const freq = fractionToFreq(i / points);
    line.push([freqX(view, freq), multibandY(view, bandLevelsDb(s, freq)[band] ?? FLOOR_DB)]);
  }
  return linePath(line);
}

/** Where band `band` lies along the axis: from the split below it to the split above it (the axis ends outside). */
export function bandSpan(s: MultibandSettings, band: number, view: PlotView): { from: number; to: number } {
  const edges = [FREQ_MIN_HZ, ...s.splits.slice(0, s.count - 1), FREQ_MAX_HZ];
  return { from: freqX(view, edges[band]!), to: freqX(view, edges[band + 1]!) };
}

/** What dragging split line `split` to x `px` of the view sets: its frequency, within its param range. */
export function dragSplit(s: MultibandSettings, split: number, px: number, view: PlotView): ParamUpdate {
  const info = s.splitInfos[split]!;
  return { name: info.name, value: clamp(fractionToFreq(px / view.width), info.min, info.max) };
}

/** The param names of band `band`'s solo and bypass switches (numbered from 1 in the effect). */
export const bandSwitch = (band: number, what: "solo" | "bypass") => `band${band + 1}_${what}`;
