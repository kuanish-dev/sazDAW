import { STEP_TICKS, type NewNote, type Note, type ScaleKey, type Variation } from "../api/types";
import { inScale, random, roundAway } from "./noteTransforms";

/** How many random numbers each note of the group draws — always, with any settings. */
const DRAWS_PER_NOTE = 6;
const VELOCITY_SPREAD = 32;

/** Result: the group's notes in order (`null` — removed or merged) and the new notes. */
export interface Varied {
  kept: (Note | null)[];
  added: NewNote[];
}

type Span = [pitch: number, start: number, end: number];

/** Where notes are during variation; answers whether a place is free (one pitch never overlaps, everything stays inside `span`). */
function board(others: Note[], kept: (Note | null)[], added: NewNote[], span: number) {
  const occupied = (skip: number | null): Span[] => [
    ...others.map((n): Span => [n.pitch, n.startTick, n.startTick + n.lengthTicks]),
    ...kept.flatMap((n, i): Span[] => (n && i !== skip ? [[n.pitch, n.startTick, n.startTick + n.lengthTicks]] : [])),
    ...added.map((n): Span => [n.pitch, n.startTick, n.startTick + n.lengthTicks]),
  ];
  const isFree = (note: Note, skip: number) => {
    const end = note.startTick + note.lengthTicks;
    return end <= span && !occupied(skip).some(([pitch, start, e]) => pitch === note.pitch && start < end && note.startTick < e);
  };
  return { occupied, isFree };
}

/** Neighbouring pitch up or down: a scale degree or the nearest of the group's pitches; at the edge — the other way. */
function neighbour(pitch: number, up: boolean, scale: ScaleKey | undefined, pool: number[]): number | undefined {
  const toward = (up: boolean): number | undefined => {
    if (scale) {
      for (let d = 1; d <= 12; d++) {
        const p = pitch + (up ? d : -d);
        if (p < 0 || p > 127) return undefined;
        if (inScale(p, scale.root, scale.scale)) return p;
      }
      return undefined;
    }
    return up ? pool.find((p) => p > pitch) : [...pool].reverse().find((p) => p < pitch);
  };
  return toward(up) ?? toward(!up);
}

export function varyGroup(group: Note[], others: Note[], v: Variation, seed: number, barTicks: number, span: number): Varied {
  const rng = random(seed);
  const bar = Math.max(barTicks, 1);
  const kept: (Note | null)[] = group.map((n) => ({ ...n }));
  const added: NewNote[] = [];
  const { occupied, isFree } = board(others, kept, added, span);
  const pool = [...new Set(group.map((n) => n.pitch))].sort((a, b) => a - b);
  const end = (n: Note) => n.startTick + n.lengthTicks;

  const changeRhythm = (i: number, note: Note, kind: number): Note => {
    if (kind < 0.5) {
      const start = kind < 0.25 ? note.startTick - STEP_TICKS : note.startTick + STEP_TICKS;
      if (start < 0) return note;
      const moved = { ...note, startTick: start };
      return isFree(moved, i) ? moved : note;
    }
    if (kind < 0.75) {
      if (note.lengthTicks < 2 * STEP_TICKS) return note;
      const half = Math.floor(note.lengthTicks / 2 / STEP_TICKS) * STEP_TICKS;
      added.push({ pitch: note.pitch, startTick: note.startTick + half, lengthTicks: note.lengthTicks - half, velocity: note.velocity });
      return { ...note, lengthTicks: half };
    }
    let next: [number, Note] | undefined;
    kept.forEach((n, j) => {
      if (!n || j === i || n.pitch !== note.pitch || n.startTick % bar === 0) return;
      if (n.startTick < end(note) || n.startTick - end(note) > STEP_TICKS) return;
      if (!next || n.startTick < next[1].startTick) next = [j, n];
    });
    if (!next) return note;
    const [j, found] = next;
    kept[j] = null;
    const merged = { ...note, lengthTicks: end(found) - note.startTick };
    if (isFree(merged, i)) return merged;
    kept[j] = found;
    return note;
  };

  for (let i = 0; i < group.length; i++) {
    const draw = Array.from({ length: DRAWS_PER_NOTE }, () => rng.unit());
    const current = kept[i];
    if (!current) continue;
    let note = current;
    const downbeat = note.startTick % bar === 0;
    const remove = downbeat ? -v.density - 0.5 : -v.density;
    if (draw[0]! < remove) {
      kept[i] = null;
      continue;
    }
    if (!downbeat) {
      if (draw[1]! < v.pitch) {
        const pitch = neighbour(note.pitch, draw[2]! < 0.5, v.scale, pool);
        if (pitch !== undefined) {
          const moved = { ...note, pitch };
          if (isFree(moved, i)) note = moved;
        }
      }
      if (draw[3]! < v.rhythm) {
        kept[i] = note;
        note = changeRhythm(i, note, draw[4]!);
      }
    }
    const shift = roundAway((draw[5]! * 2 - 1) * v.velocity * VELOCITY_SPREAD);
    note = { ...note, velocity: Math.min(Math.max(note.velocity + shift, 1), 127) };
    kept[i] = note;
  }

  if (v.density > 0) addNotes(roundAway(v.density * group.length));
  return { kept, added };

  function addNotes(count: number) {
    const starts = new Set([...kept.flatMap((n) => (n ? [n.startTick] : [])), ...added.map((n) => n.startTick)]);
    const free: number[] = [];
    for (let s = 0; s < Math.floor(span / STEP_TICKS); s++) if (!starts.has(s * STEP_TICKS)) free.push(s * STEP_TICKS);
    for (let k = 0; k < count; k++) {
      const slot = rng.unit();
      const pick = rng.unit();
      if (free.length === 0 || group.length === 0) break;
      const start = free.splice(Math.floor(slot * free.length), 1)[0]!;
      const model = group[Math.floor(pick * group.length)]!;
      const same = occupied(null).filter((n) => n[0] === model.pitch);
      if (same.some(([, s, e]) => s <= start && start < e)) continue;
      const next = Math.min(...same.map((n) => n[1]).filter((s) => s > start), span);
      const length = Math.min(model.lengthTicks, next - start, span - start);
      added.push({ pitch: model.pitch, startTick: start, lengthTicks: length, velocity: model.velocity });
    }
  }
}
