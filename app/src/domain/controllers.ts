// MIDI controller lanes of a note clip (pitch bend, CC, aftertouch): kinds and their labels, value limits and point
// editing by drawing — the pencil stroke is shared with automation patterns. A point holds its value as a step until the
// next one (that's how the engine plays them). Pure functions.
import type { ControllerKind, ControllerLane, ControllerPoint, NoteContent } from "../api/types";
import { t } from "../i18n";

/** Preset kinds in the piano roll lane list; "Other CC…" adds any number. */
export const CONTROLLER_PRESETS: readonly ControllerKind[] = [
  { type: "pitchBend" },
  { type: "cc", number: 1 },
  { type: "cc", number: 64 },
  { type: "cc", number: 11 },
  { type: "aftertouch" },
];

/** Highest regular CC number (120–127 are channel mode messages). */
export const MAX_CC_NUMBER = 119;

export function controllerLabel(kind: ControllerKind): string {
  if (kind.type === "pitchBend") return t().automation.pitchBend;
  if (kind.type === "aftertouch") return t().automation.aftertouch;
  const names: Record<number, string> = t().automation.ccNames;
  const name = names[kind.number];
  return name ? `${name} (CC${kind.number})` : `CC${kind.number}`;
}

/** Kind key for lists and comparison: `pitchBend`, `aftertouch`, `cc:64`. */
export const controllerKey = (kind: ControllerKind): string => (kind.type === "cc" ? `cc:${kind.number}` : kind.type);

export const sameKind = (a: ControllerKind, b: ControllerKind): boolean => controllerKey(a) === controllerKey(b);

/** Kind from a `controllerKey` key; an unknown key — `null`. */
export function controllerFromKey(key: string): ControllerKind | null {
  if (key === "pitchBend" || key === "aftertouch") return { type: key };
  const cc = /^cc:(\d+)$/.exec(key);
  const number = cc ? Number(cc[1]) : Number.NaN;
  return number >= 0 && number <= MAX_CC_NUMBER ? { type: "cc", number } : null;
}

/** Value limits: pitch bend −1…1 (0 — wheel centered), others 0…1. */
export const controllerRange = (kind: ControllerKind): [number, number] => (kind.type === "pitchBend" ? [-1, 1] : [0, 1]);

export const lanesOf = (content: NoteContent | undefined): ControllerLane[] => content?.controllers ?? [];

export const lanePoints = (content: NoteContent | undefined, kind: ControllerKind): ControllerPoint[] =>
  lanesOf(content).find((lane) => sameKind(lane.kind, kind))?.points ?? [];

/** Kinds in the list: presets plus those that already have points in the clip (custom CC), without repeats. */
export function laneChoices(content: NoteContent | undefined, extra: ControllerKind | null): ControllerKind[] {
  const all = [...CONTROLLER_PRESETS, ...lanesOf(content).map((lane) => lane.kind), ...(extra ? [extra] : [])];
  return all.filter((kind, i) => all.findIndex((other) => sameKind(other, kind)) === i);
}

/**
 * Result of drawing: lane points minus those inside the dragged span `[from, to]` (either order), plus the drawn
 * `stroke`; sorted by tick, the drawn one wins on the same tick.
 */
export function drawStroke(points: ControllerPoint[], from: number, to: number, stroke: ControllerPoint[]): ControllerPoint[] {
  const [lo, hi] = [Math.min(from, to), Math.max(from, to)];
  const ticks = new Set(stroke.map((p) => p.tick));
  const kept = points.filter((p) => (p.tick < lo || p.tick > hi) && !ticks.has(p.tick));
  return [...kept, ...stroke].sort((a, b) => a.tick - b.tick);
}

/** A pencil gesture over step points: the covered span `lo…hi`, where the pen is now, and what it drew (tick → point). */
export interface PencilStroke {
  lo: number;
  hi: number;
  last: ControllerPoint;
  drawn: ReadonlyMap<number, ControllerPoint>;
}

export const startStroke = (at: ControllerPoint): PencilStroke => ({ lo: at.tick, hi: at.tick, last: at, drawn: new Map([[at.tick, at]]) });

/** The stroke after the pen moved to `at`: the straight line from where it was is drawn at every `step` too. */
export function extendStroke(stroke: PencilStroke, at: ControllerPoint, step: number): PencilStroke {
  const drawn = new Map(stroke.drawn);
  for (const p of strokeSegment(stroke.last.tick, stroke.last.value, at.tick, at.value, step)) drawn.set(p.tick, p);
  return { lo: Math.min(stroke.lo, at.tick), hi: Math.max(stroke.hi, at.tick), last: at, drawn };
}

/** `points` with the stroke over them: everything in the covered span is replaced (the pen may have gone forward and back). */
export const strokeOver = (points: ControllerPoint[], stroke: PencilStroke): ControllerPoint[] =>
  drawStroke(points, stroke.lo, stroke.hi, [...stroke.drawn.values()]);

/** Points without the one nearest to `tick` within `tolerance` ticks; none nearby — the same points. */
export function removeNear(points: ControllerPoint[], tick: number, tolerance: number): ControllerPoint[] {
  let best = -1;
  points.forEach((p, i) => {
    if (Math.abs(p.tick - tick) <= tolerance && (best < 0 || Math.abs(p.tick - tick) < Math.abs(points[best]!.tick - tick))) best = i;
  });
  return best < 0 ? points : points.filter((_, i) => i !== best);
}

/** Lane height fraction 0…1 for a value of `kind` (bottom — minimum). */
export function toFraction(kind: ControllerKind, value: number): number {
  const [min, max] = controllerRange(kind);
  return (value - min) / (max - min);
}

/**
 * Value of `kind` from a height fraction, in MIDI steps: 127 for CC and aftertouch, 128 for pitch bend so the
 * middle (wheel at rest, 0) is hit exactly.
 */
export function fromFraction(kind: ControllerKind, fraction: number): number {
  const [min, max] = controllerRange(kind);
  const steps = kind.type === "pitchBend" ? 128 : 127;
  const clamped = Math.min(Math.max(fraction, 0), 1);
  return min + (Math.round(clamped * steps) / steps) * (max - min);
}

/** Value for display: CC and aftertouch 0…127, pitch bend −100…+100 %. */
export function formatControllerValue(kind: ControllerKind, value: number): string {
  if (kind.type === "pitchBend") return `${value > 0 ? "+" : ""}${Math.round(value * 100)}%`;
  return String(Math.round(value * 127));
}

/**
 * Drawn points between two mouse positions at every `step` (from `fromTick`, both ticks on the grid):
 * the value changes linearly so a fast gesture leaves no gaps.
 */
export function strokeSegment(fromTick: number, fromValue: number, toTick: number, toValue: number, step: number): ControllerPoint[] {
  const direction = toTick >= fromTick ? 1 : -1;
  const count = Math.round(Math.abs(toTick - fromTick) / step);
  return Array.from({ length: count + 1 }, (_, i) => ({
    tick: fromTick + direction * i * step,
    value: count === 0 ? toValue : fromValue + ((toValue - fromValue) * i) / count,
  }));
}
