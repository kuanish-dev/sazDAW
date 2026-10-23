import type { ParamInfo } from "../api/types";
import { clamp } from "../common/math";

/**
 * Envelope by parameter naming convention: attack, hold, decay, sustain, release.
 * Any instrument that exposes them gets a graph; attack and release are required.
 */
export interface EnvelopeSpec {
  attack: ParamInfo;
  release: ParamInfo;
  hold?: ParamInfo;
  decay?: ParamInfo;
  sustain?: ParamInfo;
}

export type EnvelopeValues = Record<string, number>;

export function envelopeSpec(infos: ParamInfo[]): EnvelopeSpec | null {
  const byName = (name: string) => infos.find((p) => p.name === name);
  const attack = byName("attack");
  const release = byName("release");
  if (!attack || !release) return null;
  return { attack, release, hold: byName("hold"), decay: byName("decay"), sustain: byName("sustain") };
}

/** Soft logarithm: distinguishes both milliseconds and seconds well, and handles zero. */
const SOFT = 0.003;

export function timeToFraction(time: number, info: ParamInfo): number {
  const clamped = clamp(time, 0, info.max);
  return Math.log1p(clamped / SOFT) / Math.log1p(info.max / SOFT);
}

export function fractionToTime(fraction: number, info: ParamInfo): number {
  const f = clamp(fraction, 0, 1);
  const time = SOFT * Math.expm1(f * Math.log1p(info.max / SOFT));
  return clamp(time, info.min, info.max);
}

export interface Point {
  x: number;
  y: number;
}

export interface EnvelopeLayout {
  start: Point;
  attack: Point;
  hold?: Point;
  decay?: Point;
  /** End of the sustain plateau (or the last point before release). */
  sustain: Point;
  release: Point;
  /** Width taken by one time segment at its maximum value. */
  segment: number;
  top: number;
  bottom: number;
  path: string;
}

const SUSTAIN_FLAT = 0.14;
/** Minimum visible segment width: handles of adjacent stages do not overlap even at zero time. */
const MIN_STAGE = 12;

export function layoutEnvelope(values: EnvelopeValues, spec: EnvelopeSpec, width: number, height: number, pad = 10): EnvelopeLayout {
  const inner = width - pad * 2;
  const timed = 2 + (spec.hold ? 1 : 0) + (spec.decay ? 1 : 0);
  const flat = inner * SUSTAIN_FLAT;
  const segment = (inner - flat) / timed;
  const top = pad;
  const bottom = height - pad;
  const span = bottom - top;

  const width_ = (info: ParamInfo, name: string) =>
    MIN_STAGE + (segment - MIN_STAGE) * timeToFraction(values[name] ?? info.default, info);
  const sustainLevel = spec.sustain ? clamp(values.sustain ?? spec.sustain.default, 0, 1) : 1;

  const start = { x: pad, y: bottom };
  const attack = { x: start.x + width_(spec.attack, "attack"), y: top };
  const hold = spec.hold ? { x: attack.x + width_(spec.hold, "hold"), y: top } : undefined;
  const afterHold = hold ?? attack;
  const decay = spec.decay
    ? { x: afterHold.x + width_(spec.decay, "decay"), y: bottom - sustainLevel * span }
    : undefined;
  const levelPoint = decay ?? afterHold;
  const sustain = { x: levelPoint.x + flat, y: spec.sustain ? bottom - sustainLevel * span : levelPoint.y };
  const release = { x: sustain.x + width_(spec.release, "release"), y: bottom };

  const points = [start, attack, hold, decay, sustain, release].filter((p): p is Point => p !== undefined);
  const path = points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(" ");
  return { start, attack, hold, decay, sustain, release, segment, top, bottom, path };
}

export type Handle = "attack" | "hold" | "decay" | "sustain" | "release";

export interface ParamUpdate {
  name: string;
  value: number;
}

/** What to change when handle `handle` is dragged to point (x, y). */
export function dragEnvelope(handle: Handle, x: number, y: number, layout: EnvelopeLayout, spec: EnvelopeSpec): ParamUpdate[] {
  const fraction = (from: number) => (x - from - MIN_STAGE) / (layout.segment - MIN_STAGE);
  const level = clamp((layout.bottom - y) / (layout.bottom - layout.top), 0, 1);
  switch (handle) {
    case "attack":
      return [{ name: "attack", value: fractionToTime(fraction(layout.start.x), spec.attack) }];
    case "hold":
      return spec.hold && layout.hold ? [{ name: "hold", value: fractionToTime(fraction(layout.attack.x), spec.hold) }] : [];
    case "decay": {
      if (!spec.decay || !layout.decay) return [];
      const from = (layout.hold ?? layout.attack).x;
      const updates = [{ name: "decay", value: fractionToTime(fraction(from), spec.decay) }];
      if (spec.sustain) updates.push({ name: "sustain", value: level });
      return updates;
    }
    case "sustain":
      return spec.sustain ? [{ name: "sustain", value: level }] : [];
    case "release":
      return [{ name: "release", value: fractionToTime(fraction(layout.sustain.x), spec.release) }];
  }
}
