// EQ graph: the frequency response of the built-in EQ (`crates/daw-effects/src/eq.rs`) on a log-frequency axis and what
// dragging its band handles changes. The filters MIRROR `crates/daw-effects/src/biquad.rs` (RBJ Audio EQ Cookbook) and
// the band rules of `eq.rs` (a cut at its extreme frequency is off): a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import type { ParamUpdate } from "./catalog";
import { fractionToFreq, freqX } from "./frequencyAxis";
import { clamp } from "./math";
import type { PlotView } from "./plot";

/** The vertical range of the graph, ± dB (the gain range of the bands). */
export const EQ_RANGE_DB = 18;
/** Sample rate for drawing: the response near the top of the band differs slightly at other rates. */
const DRAW_RATE = 48_000;
/** Butterworth Q of the 12 dB/oct cuts (`BUTTERWORTH_Q`). */
const BUTTERWORTH_Q = Math.SQRT1_2;

interface Coefficients {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

const PASS: Coefficients = { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };

function normalized(b0: number, b1: number, b2: number, a0: number, a1: number, a2: number): Coefficients {
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

/** `side` = 1 low shelf, −1 high shelf (as `Coefficients::shelf`). */
function shelf(freq: number, gainDb: number, side: number): Coefficients {
  const a = 10 ** (gainDb / 40);
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const cos = side * Math.cos(w);
  const beta = 2 * Math.sqrt(a) * ((Math.sin(w) / 2) * Math.SQRT2);
  return normalized(
    a * (a + 1 - (a - 1) * cos + beta),
    side * 2 * a * (a - 1 - (a + 1) * cos),
    a * (a + 1 - (a - 1) * cos - beta),
    a + 1 + (a - 1) * cos + beta,
    side * -2 * (a - 1 + (a + 1) * cos),
    a + 1 + (a - 1) * cos - beta,
  );
}

function peaking(freq: number, gainDb: number, q: number): Coefficients {
  const a = 10 ** (gainDb / 40);
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const alpha = Math.sin(w) / (2 * q);
  const cos = Math.cos(w);
  return normalized(1 + alpha * a, -2 * cos, 1 - alpha * a, 1 + alpha / a, -2 * cos, 1 - alpha / a);
}

/** `high` = true high-pass, false low-pass. */
function pass(freq: number, q: number, high: boolean): Coefficients {
  const w = (2 * Math.PI * Math.min(freq, DRAW_RATE * 0.45)) / DRAW_RATE;
  const cos = Math.cos(w);
  const alpha = Math.sin(w) / (2 * q);
  const edge = high ? (1 + cos) / 2 : (1 - cos) / 2;
  return normalized(edge, high ? -(1 + cos) : 1 - cos, edge, 1 + alpha, -2 * cos, 1 - alpha);
}

/** Response of one biquad at `freq`, in dB. */
function magnitudeDb(c: Coefficients, freq: number): number {
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const [cos1, sin1, cos2, sin2] = [Math.cos(w), Math.sin(w), Math.cos(2 * w), Math.sin(2 * w)];
  const num = (c.b0 + c.b1 * cos1 + c.b2 * cos2) ** 2 + (c.b1 * sin1 + c.b2 * sin2) ** 2;
  const den = (1 + c.a1 * cos1 + c.a2 * cos2) ** 2 + (c.a1 * sin1 + c.a2 * sin2) ** 2;
  return 10 * Math.log10(Math.max(num, 1e-20) / Math.max(den, 1e-20));
}

const EQ_PARAMS = ["low_cut", "low_gain", "low_freq", "mid_gain", "mid_freq", "mid_q", "high_gain", "high_freq", "high_cut"] as const;
type EqParam = (typeof EQ_PARAMS)[number];

/** The EQ params by name with the catalog's ranges. */
export interface EqSpec {
  infos: Record<EqParam, ParamInfo>;
}

/** The spec by the EQ's param names; `null` when some are missing (not this EQ). */
export function eqSpec(infos: ParamInfo[]): EqSpec | null {
  const byName: Partial<Record<EqParam, ParamInfo>> = {};
  for (const name of EQ_PARAMS) {
    const info = infos.find((p) => p.name === name);
    if (!info) return null;
    byName[name] = info;
  }
  return { infos: byName as Record<EqParam, ParamInfo> };
}

const value = (spec: EqSpec, values: Params, name: EqParam) => values[name] ?? spec.infos[name].default;

/** Whether a cut is off: at its extreme frequency (`low_cut` at its minimum, `high_cut` at its maximum), as in `eq.rs`. */
export function cutIsOff(spec: EqSpec, values: Params, name: "low_cut" | "high_cut"): boolean {
  const v = value(spec, values, name);
  return name === "low_cut" ? v <= spec.infos.low_cut.min : v >= spec.infos.high_cut.max;
}

function filters(spec: EqSpec, values: Params): Coefficients[] {
  const v = (name: EqParam) => value(spec, values, name);
  return [
    cutIsOff(spec, values, "low_cut") ? PASS : pass(v("low_cut"), BUTTERWORTH_Q, true),
    shelf(v("low_freq"), v("low_gain"), 1),
    peaking(v("mid_freq"), v("mid_gain"), v("mid_q")),
    shelf(v("high_freq"), v("high_gain"), -1),
    cutIsOff(spec, values, "high_cut") ? PASS : pass(v("high_cut"), BUTTERWORTH_Q, false),
  ];
}

/** Total response of the EQ at `freq`, in dB. */
export function eqResponseDb(spec: EqSpec, values: Params, freq: number): number {
  return filters(spec, values).reduce((sum, c) => sum + magnitudeDb(c, freq), 0);
}

const y = (view: PlotView, db: number) => ((EQ_RANGE_DB - clamp(db, -EQ_RANGE_DB, EQ_RANGE_DB)) / (2 * EQ_RANGE_DB)) * view.height;

/** The SVG path of the response curve. */
export function eqCurvePath(spec: EqSpec, values: Params, view: PlotView, points = 160): string {
  const fs = filters(spec, values);
  const parts: string[] = [];
  for (let i = 0; i <= points; i++) {
    const freq = fractionToFreq(i / points);
    const db = fs.reduce((sum, c) => sum + magnitudeDb(c, freq), 0);
    parts.push(`${i === 0 ? "M" : "L"}${freqX(view, freq).toFixed(1)} ${y(view, db).toFixed(1)}`);
  }
  return parts.join(" ");
}

export type EqHandle = "low_cut" | "low" | "mid" | "high" | "high_cut";

/** Which params a handle moves: frequency always, gain for the tone bands (Q of the mid band goes by wheel, `wheelQ`). */
export const HANDLE_PARAMS: Record<EqHandle, { freq: EqParam; gain?: EqParam }> = {
  low_cut: { freq: "low_cut" },
  low: { freq: "low_freq", gain: "low_gain" },
  mid: { freq: "mid_freq", gain: "mid_gain" },
  high: { freq: "high_freq", gain: "high_gain" },
  high_cut: { freq: "high_cut" },
};

export interface EqHandlePoint {
  id: EqHandle;
  x: number;
  y: number;
  /** A cut that is off (drawn dimmed at its extreme). */
  off: boolean;
}

/** Handle positions: tone bands at (frequency, gain), cuts at their frequency on the curve. */
export function eqHandles(spec: EqSpec, values: Params, view: PlotView): EqHandlePoint[] {
  return (Object.keys(HANDLE_PARAMS) as EqHandle[]).map((id) => {
    const { freq, gain } = HANDLE_PARAMS[id];
    const f = value(spec, values, freq);
    const off = (id === "low_cut" || id === "high_cut") && cutIsOff(spec, values, id);
    const db = gain ? value(spec, values, gain) : off ? 0 : eqResponseDb(spec, values, f);
    return { id, x: freqX(view, f), y: y(view, db), off };
  });
}

/** What dragging `handle` to point (px, py) of the view sets: the frequency (and gain for a tone band), within the catalog ranges. */
export function dragEqHandle(spec: EqSpec, handle: EqHandle, px: number, py: number, view: PlotView): ParamUpdate[] {
  const { freq, gain } = HANDLE_PARAMS[handle];
  const fInfo = spec.infos[freq];
  const updates = [{ name: freq, value: clamp(fractionToFreq(px / view.width), fInfo.min, fInfo.max) }];
  if (gain) {
    const gInfo = spec.infos[gain];
    const db = EQ_RANGE_DB - (clamp(py, 0, view.height) / view.height) * 2 * EQ_RANGE_DB;
    updates.push({ name: gain, value: clamp(db, gInfo.min, gInfo.max) });
  }
  return updates;
}

/** Wheel over the mid handle: Q grows (narrower) on wheel up, by a factor per notch, within its range. */
export function wheelQ(spec: EqSpec, values: Params, deltaY: number): ParamUpdate {
  const info = spec.infos.mid_q;
  return { name: info.name, value: clamp(value(spec, values, "mid_q") * Math.exp(-deltaY * 0.002), info.min, info.max) };
}

/** Grid lines of the gain axis (the frequency grid is the axis's own, `FREQ_GRID_HZ`) and their positions. */
export const EQ_GRID_DB = [-12, -6, 0, 6, 12];
export const gridY = (view: PlotView, db: number) => y(view, db);
