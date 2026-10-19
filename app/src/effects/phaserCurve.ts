// Phaser graph (`effects/PhaserGraph.tsx`): the response of the dry sound mixed with the all-pass chain, per channel,
// at the LFO's position now. MIRRORS `crates/daw-effects/src/phaser.rs` (first-order all-passes with the coefficient
// `(tan(πf/fs) − 1) / (tan(πf/fs) + 1)`, 4/6/8/12 `stages`, every corner swept evenly in octaves between `low` and `high`
// by a sine LFO at `rate` that starts at the low end, the right channel `spread` degrees later, `feedback` around the
// chain, `mix`): a change there must be repeated here. The engine does not report its LFO phase, so the picture turns its
// own at the same rate: right speed and range, not the engine's exact moment.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import { FREQ_MAX_HZ, FREQ_MIN_HZ, freqX } from "./frequencyAxis";
import { clamp } from "../common/math";
import { type PlotView, type Point, dbY, linePath, toDb } from "../common/plot";

/** Stage counts in the order of the `stages` choice. */
const STAGES = [4, 6, 8, 12] as const;
/** The rate the picture computes the digital filters at (the response barely differs at other rates in the audible band). */
const SAMPLE_RATE = 48_000;
export const PHASER_TOP_DB = 12;
export const PHASER_FLOOR_DB = -36;
export const PHASER_GRID_DB = [6, 0, -12, -24] as const;

export interface PhaserSettings {
  rate: number;
  low: number;
  high: number;
  stages: number;
  feedback: number;
  spread: number;
  mix: number;
}

export const phaserSettings = (infos: ParamInfo[], values: Params): PhaserSettings | null =>
  namedParams(infos, values, ["rate", "low", "high", "stages", "feedback", "spread", "mix"]);

/** The channels: 0 the left, 1 the right (`spread` later). */
export type Channel = 0 | 1;

/** The LFO position (cycles) `dt` seconds later. */
export const advancePhase = (phase: number, rate: number, dt: number) => (phase + rate * dt) % 1;

/** The corner of every stage of `channel` at LFO position `phase` (cycles). */
export function cornerHz(s: PhaserSettings, phase: number, channel: Channel): number {
  const sweep = 0.5 - 0.5 * Math.cos(2 * Math.PI * (phase + (channel * s.spread) / 360));
  return 2 ** (Math.log2(s.low) + sweep * (Math.log2(s.high) - Math.log2(s.low)));
}

/** The phaser's gain (linear) at `hz` with every stage's corner at `corner`. */
export function phaserGain(s: PhaserSettings, corner: number, hz: number): number {
  const stages = STAGES[clamp(Math.round(s.stages), 0, STAGES.length - 1)]!;
  const t = Math.tan((Math.PI * Math.min(corner, 0.45 * SAMPLE_RATE)) / SAMPLE_RATE);
  const a = (t - 1) / (t + 1);
  const w = (2 * Math.PI * hz) / SAMPLE_RATE;
  // One stage, (a + e^−jw) / (1 + a·e^−jw), has a level of 1 and the angle below; the chain turns `stages` times as far.
  const angle = stages * Math.atan2(-Math.sin(w) * (1 - a * a), 2 * a + Math.cos(w) * (1 + a * a));
  const [chainRe, chainIm] = [Math.cos(angle), Math.sin(angle)];
  // With feedback: wet = chain / (1 − feedback · chain).
  const [qRe, qIm] = [1 - s.feedback * chainRe, -s.feedback * chainIm];
  const q = qRe * qRe + qIm * qIm;
  const [wetRe, wetIm] = [(chainRe * qRe + chainIm * qIm) / q, (chainIm * qRe - chainRe * qIm) / q];
  return Math.hypot(1 - s.mix + s.mix * wetRe, s.mix * wetIm);
}

/** The y of a level in dB in the view. */
export const phaserY = (view: PlotView, db: number) => dbY(view, db, PHASER_FLOOR_DB, PHASER_TOP_DB);

/** The response of `channel` at LFO position `phase` from 20 Hz to 20 kHz. */
export function phaserPath(s: PhaserSettings, phase: number, channel: Channel, view: PlotView, points = 200): string {
  const corner = cornerHz(s, phase, channel);
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const hz = FREQ_MIN_HZ * (FREQ_MAX_HZ / FREQ_MIN_HZ) ** (i / points);
    line.push([freqX(view, hz), phaserY(view, toDb(phaserGain(s, corner, hz)))]);
  }
  return linePath(line);
}
