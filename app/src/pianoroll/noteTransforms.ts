// Musical edits of a group of notes — an exact copy of the backend rules (`daw-project/src/note_transforms.rs`): the mock
// answers with them, and the interface takes the scale list and degree highlighting from here. Integer rounding and the
// mulberry32 generator match Rust, so one humanize seed gives the same result in the mock too.
import { PPQ, type Note, type Scale } from "../api/types";
import { t } from "../i18n";

/** Largest humanize spread of note starts (`MAX_HUMANIZE_TICKS`): a quarter note. */
export const MAX_HUMANIZE_TICKS = PPQ;
export const MAX_HUMANIZE_VELOCITY = 126;

/** Scale degrees in semitones from the root (`Scale::intervals`). */
const INTERVALS: Readonly<Record<Scale, readonly number[]>> = {
  major: [0, 2, 4, 5, 7, 9, 11],
  minor: [0, 2, 3, 5, 7, 8, 10],
  dorian: [0, 2, 3, 5, 7, 9, 10],
  phrygian: [0, 1, 3, 5, 7, 8, 10],
  lydian: [0, 2, 4, 6, 7, 9, 11],
  mixolydian: [0, 2, 4, 5, 7, 9, 10],
  harmonicMinor: [0, 2, 3, 5, 7, 8, 11],
  pentatonicMajor: [0, 2, 4, 7, 9],
  pentatonicMinor: [0, 3, 5, 7, 10],
  chromatic: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
} as const;

/** Scales in display order; labels are `t().noteTools.scales[scale]`. */
export const SCALES: readonly Scale[] = [
  "major", "minor", "dorian", "phrygian", "lydian", "mixolydian", "harmonicMinor", "pentatonicMajor", "pentatonicMinor", "chromatic",
] as const;

/** The chosen scale: root (0 = C) and scale; used by "Fit to scale" and piano roll row highlighting. */
export interface ScaleChoice {
  root: number;
  scale: Scale;
}

export const isScale = (value: string): value is Scale => value in INTERVALS;

/** Whether the pitch belongs to the scale with root `root` (0 = C). */
export const inScale = (pitch: number, root: number, scale: Scale): boolean =>
  INTERVALS[scale].includes((((pitch - root) % 12) + 12) % 12);

/** Nearest scale pitch within 0…127; at equal distance — the lower one. */
export function nearestInScale(pitch: number, root: number, scale: Scale): number {
  for (let distance = 0; distance < 12; distance++) {
    if (pitch - distance >= 0 && inScale(pitch - distance, root, scale)) return pitch - distance;
    if (pitch + distance <= 127 && inScale(pitch + distance, root, scale)) return pitch + distance;
  }
  return pitch;
}

/** A note inside content of length `limit`: length from one tick up to the content, anything past the end moves back. */
export function fit(note: Note, start: number, length: number, limit: number): Note {
  const len = Math.min(Math.max(length, 1), limit);
  return { ...note, startTick: Math.min(Math.max(start, 0), limit - len), lengthTicks: len };
}

const nearestLine = (tick: number, grid: number) => Math.floor((tick + Math.floor(grid / 2)) / grid) * grid;
/** Rounds half away from zero, like `f64::round` in Rust (`Math.round` pulls −0.5 up). */
export const roundAway = (value: number) => Math.sign(value) * Math.round(Math.abs(value));
const toward = (from: number, to: number, strength: number) => from + roundAway((to - from) * strength);

export function quantizeGroup(group: Note[], grid: number, strength: number, ends: boolean, limit: number): Note[] {
  return group.map((note) => {
    const end = note.startTick + note.lengthTicks;
    const line = nearestLine(note.startTick, grid);
    const start = toward(note.startTick, line, strength);
    const length = ends ? toward(end, Math.max(nearestLine(end, grid), line + grid), strength) - start : note.lengthTicks;
    return fit(note, start, length, limit);
  });
}

/** mulberry32, the same as in the backend (`Random`): `offset` is an integer from −range to range, `unit` is from 0 to 1 (exclusive). */
export function random(seed: number) {
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (t ^ (t >>> 14)) >>> 0;
  };
  return {
    offset: (range: number) => (next() % (2 * range + 1)) - range,
    unit: () => next() / 4294967296,
  };
}

export function humanizeGroup(group: Note[], timingTicks: number, velocity: number, seed: number, limit: number): Note[] {
  const rng = random(seed);
  return group.map((note) => {
    // Both numbers are always drawn: one seed gives the same offsets with any settings.
    const dt = rng.offset(timingTicks);
    const dv = rng.offset(velocity);
    const v = Math.min(Math.max(note.velocity + dv, 1), 127);
    return fit({ ...note, velocity: v }, note.startTick + dt, note.lengthTicks, limit);
  });
}

/** Transposition; a note outside 0…127 is an error (worded like the backend) and nothing changes. */
export function transposeGroup(group: Note[], semitones: number): Note[] {
  return group.map((note) => {
    const pitch = note.pitch + semitones;
    if (pitch < 0 || pitch > 127) {
      throw t().noteTools.errorTranspose(semitones, note.id, note.pitch);
    }
    return { ...note, pitch };
  });
}

export const fitGroupToScale = (group: Note[], root: number, scale: Scale): Note[] =>
  group.map((note) => ({ ...note, pitch: nearestInScale(note.pitch, root, scale) }));

export function legatoGroup(group: Note[]): Note[] {
  const starts = [...new Set(group.map((n) => n.startTick))].sort((a, b) => a - b);
  return group.map((note) => {
    const next = starts.find((s) => s > note.startTick);
    return next === undefined ? note : { ...note, lengthTicks: next - note.startTick };
  });
}
