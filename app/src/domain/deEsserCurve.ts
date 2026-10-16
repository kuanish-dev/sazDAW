// The de-esser's graph (`ui/effects/DeEsserGraph.tsx`): the band it listens to and the response it makes for a gain
// reduction. MIRRORS `crates/daw-effects/src/de_esser.rs`: the 0-dB band-pass at Q 1 (`biquad.ts`); split-band takes
// the reduction off the band only (`1 − (1 − g)·band`), wideband off everything (`g`), listen plays the band alone: a
// change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { bandPass, complexDb, response } from "./biquad";
import { type ParamUpdate, namedParams } from "./catalog";
import { fractionToFreq, freqX } from "./frequencyAxis";
import { clamp } from "./math";
import { type PlotView, type Point, dbY, linePath } from "./plot";

/** Width of the detected band (`BAND_Q`). */
const BAND_Q = 1;
/** The level axis: the band's top at 0 dB and room below for the deepest range (24 dB). */
const TOP_DB = 6;
const FLOOR_DB = -30;
export const DE_ESSER_GRID_DB = [0, -6, -12, -18, -24];
export const deEsserY = (view: PlotView, db: number) => dbY(view, db, FLOOR_DB, TOP_DB);

export interface DeEsserSettings {
  freq: number;
  /** The most it takes off, dB. */
  range: number;
  wideband: boolean;
  listen: boolean;
  freqInfo: ParamInfo;
}

/** The settings by the de-esser's param names; `null` when one is missing. */
export function deEsserSettings(infos: ParamInfo[], values: Params): DeEsserSettings | null {
  const s = namedParams(infos, values, ["freq", "range", "mode", "listen"]);
  const freqInfo = infos.find((p) => p.name === "freq");
  return s && freqInfo ? { freq: s.freq, range: s.range, wideband: s.mode >= 1, listen: s.listen >= 0.5, freqInfo } : null;
}

/** The response at `freq` while `reductionDb` is taken off, dB. */
function responseDb(s: DeEsserSettings, reductionDb: number, freq: number): number {
  const [re, im] = response(bandPass(s.freq, BAND_Q), freq);
  if (s.listen) return complexDb([re, im]);
  const gain = 10 ** (-reductionDb / 20);
  if (s.wideband) return 20 * Math.log10(gain);
  return complexDb([1 - (1 - gain) * re, -(1 - gain) * im]);
}

function points(level: (freq: number) => number, view: PlotView, count = 160): Point[] {
  return Array.from({ length: count + 1 }, (_, i): Point => {
    const freq = fractionToFreq(i / count);
    return [freqX(view, freq), deEsserY(view, level(freq))];
  });
}

/** The SVG path of the response while `reductionDb` is taken off. */
export const deEsserPath = (s: DeEsserSettings, reductionDb: number, view: PlotView) => linePath(points((f) => responseDb(s, reductionDb, f), view));

/** The detected band as a closed area over the bottom of the view. */
export function bandAreaPath(s: DeEsserSettings, view: PlotView): string {
  const outline = points((f) => complexDb(response(bandPass(s.freq, BAND_Q), f)), view);
  return `${linePath([[0, view.height], ...outline, [view.width, view.height]])} Z`;
}

/** What dragging the band to x `px` of the view sets: its frequency, within its range. */
export function dragDeEsser(s: DeEsserSettings, px: number, view: PlotView): ParamUpdate {
  return { name: s.freqInfo.name, value: clamp(fractionToFreq(px / view.width), s.freqInfo.min, s.freqInfo.max) };
}
