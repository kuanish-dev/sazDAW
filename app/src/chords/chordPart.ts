// Chord part, pure functions: chord symbols, a key's own chords (the palette), editing the progression bar by bar, the
// track the part plays on and arrows for a strum grid. The backend reads the symbols (`Chord::parse`) and voices them.
import type { ChordTarget, Id, Project } from "../api/types";
import { BASS_GUITAR_KIND, ELECTRIC_GUITAR_KIND, GUITAR_KIND, UKULELE_KIND } from "../instruments/strings";

/** Root names by pitch class, spelled as the backend spells them. */
export const ROOTS = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
/** Chord types as symbol suffixes: the backend's list (`ChordQuality::suffix`), major first. */
export const QUALITIES = ["", "m", "7", "maj7", "m7", "sus2", "sus4", "dim", "aug"];

export type KeyMode = "major" | "minor";

export const chordSymbol = (root: number, quality: string): string => `${ROOTS[((root % 12) + 12) % 12]}${quality}`;

/** A key's own triads in scale order: major I ii iii IV V vi vii°, natural minor i ii° III iv v VI VII. */
const DEGREES: Record<KeyMode, [number, string][]> = {
  major: [[0, ""], [2, "m"], [4, "m"], [5, ""], [7, ""], [9, "m"], [11, "dim"]],
  minor: [[0, "m"], [2, "dim"], [3, ""], [5, "m"], [7, "m"], [8, ""], [10, ""]],
};

export function keyChords(root: number, mode: KeyMode): string[] {
  return DEGREES[mode].map(([step, quality]) => chordSymbol(root + step, quality));
}

/** A first progression to start from: the popular I–V–vi–IV in major, i–VI–III–VII in minor. */
export function startingProgression(root: number, mode: KeyMode): string[][] {
  const chords = keyChords(root, mode);
  const degrees = mode === "major" ? [0, 4, 5, 3] : [0, 5, 2, 6];
  return degrees.map((degree) => [chords[degree] ?? chordSymbol(root, "")]);
}

/** A chord's place in the progression: the bar and the half (0 the whole bar or its first half, 1 the second). */
export interface ChordCell {
  bar: number;
  half: number;
}

export function setChord(bars: string[][], cell: ChordCell, symbol: string): string[][] {
  return bars.map((chords, bar) =>
    bar !== cell.bar ? chords : chords.length === 2 ? chords.map((chord, half) => (half === cell.half ? symbol : chord)) : [symbol]);
}

/** One chord in the bar becomes two (the same chord twice), two become the first one. */
export function toggleSplit(bars: string[][], bar: number): string[][] {
  return bars.map((chords, i) => {
    if (i !== bar) return chords;
    const first = chords[0] ?? "C";
    return chords.length === 2 ? [first] : [first, first];
  });
}

/** A new last bar repeats the last chord. */
export const addBar = (bars: string[][]): string[][] => [...bars, [bars.at(-1)?.at(-1) ?? "C"]];

export const removeBar = (bars: string[][], bar: number): string[][] => bars.filter((_, i) => i !== bar);

/** The cell after `cell` in reading order; the last one stays where it is. */
export function nextCell(bars: string[][], cell: ChordCell): ChordCell {
  const chords = bars[cell.bar];
  if (chords && cell.half + 1 < chords.length) return { bar: cell.bar, half: cell.half + 1 };
  return cell.bar + 1 < bars.length ? { bar: cell.bar + 1, half: 0 } : cell;
}

/** `cell` kept inside the progression after bars were removed or joined. */
export function clampCell(bars: string[][], cell: ChordCell): ChordCell {
  const bar = Math.max(0, Math.min(cell.bar, bars.length - 1));
  return { bar, half: Math.min(cell.half, (bars[bar]?.length ?? 1) - 1) };
}

/** A strum grid as arrows: ↓ down, ↑ up, ✕ muted, · nothing. */
export function strumArrows(grid: string): string[] {
  const arrows: Record<string, string> = { d: "↓", u: "↑", x: "✕" };
  return [...grid].map((symbol) => arrows[symbol.toLowerCase()] ?? "·");
}

/** Kinds a new track for the part is offered with: the strings, which strum or play roots. */
export const NEW_TRACK_KINDS = [GUITAR_KIND, ELECTRIC_GUITAR_KIND, UKULELE_KIND, BASS_GUITAR_KIND];

/** A `<select>` value of a target: `track:<id>` or `new:<kind>`. */
export const targetChoice = (target: ChordTarget): string => (target.type === "track" ? `track:${target.id}` : `new:${target.kind}`);

export function choiceTarget(choice: string): ChordTarget {
  return choice.startsWith("track:") ? { type: "track", id: Number(choice.slice("track:".length)) } : { type: "new", kind: choice.slice("new:".length) };
}

/** Where the part goes by default: the selected instrument track, otherwise a new guitar. */
export function defaultTarget(project: Project, selected: Id | null): ChordTarget {
  const track = project.tracks.find((t) => t.id === selected && t.instrument !== null);
  return track ? { type: "track", id: track.id } : { type: "new", kind: GUITAR_KIND };
}
