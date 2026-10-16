// Editing an automation clip pattern in the interface: selection moves, deletion, copy and paste. Pure functions over point
// lists; selection is a set of point indices into the list it was made on.
import type { AutoPoint } from "../api/types";
import { clamp } from "./math";

/** Copied points: times from the first copied point. */
export interface PointClip {
  points: AutoPoint[];
  /** Tick span of the copied points. */
  span: number;
}

/** Points sorted by time with the indices the given points now have. */
function sortTracking(points: AutoPoint[], tracked: AutoPoint[]): { points: AutoPoint[]; selected: Set<number> } {
  const sorted = [...points].sort((a, b) => a.tick - b.tick);
  return { points: sorted, selected: new Set(tracked.map((p) => sorted.indexOf(p)).filter((i) => i >= 0)) };
}

/** Moves the selected points by `dTick` (not before 0) and `dValue` (clamped to `min…max`), keeping the selection. */
export function moveSelected(
  points: AutoPoint[], selected: Set<number>, dTick: number, dValue: number, range: { min: number; max: number },
): { points: AutoPoint[]; selected: Set<number> } {
  const moved: AutoPoint[] = [];
  const next = points.map((p, i) => {
    if (!selected.has(i)) return p;
    const point = { ...p, tick: Math.max(0, p.tick + dTick), value: clamp(p.value + dValue, range.min, range.max) };
    moved.push(point);
    return point;
  });
  return sortTracking(next, moved);
}

export const deleteSelected = (points: AutoPoint[], selected: Set<number>): AutoPoint[] => points.filter((_, i) => !selected.has(i));

/** Indices of the points inside a tick × value rectangle. */
export function pointsInBox(points: AutoPoint[], ticks: [number, number], values: [number, number]): Set<number> {
  const [t0, t1] = [Math.min(...ticks), Math.max(...ticks)];
  const [v0, v1] = [Math.min(...values), Math.max(...values)];
  const inside = new Set<number>();
  points.forEach((p, i) => {
    if (p.tick >= t0 && p.tick <= t1 && p.value >= v0 && p.value <= v1) inside.add(i);
  });
  return inside;
}

/** Copies the selected points; `null` if nothing is selected. */
export function copySelected(points: AutoPoint[], selected: Set<number>): PointClip | null {
  const chosen = points.filter((_, i) => selected.has(i));
  const first = chosen[0];
  const last = chosen[chosen.length - 1];
  if (!first || !last) return null;
  return { points: chosen.map((p) => ({ ...p, tick: p.tick - first.tick })), span: last.tick - first.tick };
}

/** Pastes `clip` at `at`: points inside the pasted span are replaced; the pasted points come back selected. */
export function pasteAt(points: AutoPoint[], clip: PointClip, at: number): { points: AutoPoint[]; selected: Set<number> } {
  const pasted = clip.points.map((p) => ({ ...p, tick: p.tick + at }));
  const kept = points.filter((p) => p.tick < at || p.tick > at + clip.span);
  return sortTracking([...kept, ...pasted], pasted);
}
