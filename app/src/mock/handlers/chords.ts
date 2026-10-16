// Chord parts in the browser without Rust: the strums and a simplified build — the chord as a block from C3 on every
// stroke, muted strokes short (the backend voices chords per instrument, strums the strings one by one, lets the bass
// play roots and builds the part as one undo step).
import type { ChordPartRequest, Id, NewNote, StrumPattern } from "../../api/types";
import { BAR_TICKS } from "../../api/types";
import { QUALITIES } from "../../domain/chordPart";
import { barIndex, barTick } from "../../domain/tempoMap";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** The backend's strums (`daw_contracts::STRUMS`). */
const STRUM_GRIDS = { four: "D.U.X.U.", six: "D.DU.UDU", eight: "DUXUDUXU" };
type StrumId = keyof typeof STRUM_GRIDS;
const STRUM_STEP_TICKS = BAR_TICKS / 8;
/** Semitones of each chord type, in the order of `QUALITIES` (the backend's `ChordQuality::intervals`). */
const INTERVALS = [[0, 4, 7], [0, 3, 7], [0, 4, 7, 10], [0, 4, 7, 11], [0, 3, 7, 10], [0, 2, 7], [0, 5, 7], [0, 3, 6], [0, 4, 8]];
const NATURALS: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };
const C3 = 48;

const isStrum = (id: string): id is StrumId => id in STRUM_GRIDS;

/** The chord's notes from C3, or the backend's refusal for a symbol it would not read. */
function chordPitches(symbol: string): number[] {
  const text = symbol.trim();
  let root = NATURALS[text.charAt(0)];
  let rest = text.slice(1);
  if (root !== undefined && (rest.startsWith("#") || rest.startsWith("b"))) {
    root += rest.startsWith("#") ? 1 : 11;
    rest = rest.slice(1);
  }
  const intervals = INTERVALS[QUALITIES.indexOf(rest)];
  if (root === undefined || !intervals) throw fail("PT-103", { chord: symbol });
  return intervals.map((interval) => C3 + (root % 12) + interval);
}

/** One pass of the progression: every stroke plays its half bar's chord until the next stroke; a muted one is short. */
function passNotes(bars: number[][][], grid: string): NewNote[] {
  const strokes = [...grid].flatMap((symbol, step) => (symbol === "." ? [] : [{ symbol, step }]));
  return bars.flatMap((chords, bar) =>
    strokes.flatMap(({ symbol, step }, i) => {
      const chord = (step >= 4 ? chords.at(-1) : chords[0]) ?? [];
      const next = strokes[i + 1]?.step ?? 8;
      const lengthTicks = symbol.toUpperCase() === "X" ? STRUM_STEP_TICKS / 8 : (next - step) * STRUM_STEP_TICKS;
      const velocity = symbol === symbol.toUpperCase() ? 96 : 62;
      return chord.map((pitch) => ({ pitch, startTick: bar * BAR_TICKS + step * STRUM_STEP_TICKS, lengthTicks, velocity }));
    }));
}

export function chordsHandlers(w: MockWorld, h: () => HandlerSet): HandlerSet {
  return {
    "chords.strums": (): StrumPattern[] =>
      Object.entries(STRUM_GRIDS).map(([id, grid]) => ({ id, grid, name: t().mockEdits.chords.strums[id as StrumId] })),
    // The browser without Rust has no sound: preview plays nothing but answers like the backend.
    "chords.preview": () => null,
    "chords.stopPreview": () => null,
    "chords.build": ({ target, startBar = 0, strum, bars, repeat = 1, replace = false }: ChordPartRequest) => {
      if (!isStrum(strum)) throw fail("PT-074", { id: strum, ids: Object.keys(STRUM_GRIDS).join(", ") });
      if (bars.length === 0) throw fail("PT-174");
      bars.forEach((chords, i) => {
        if (chords.length < 1 || chords.length > 2) throw fail("PT-175", { bar: i + 1, count: chords.length });
      });
      const pitches = bars.map((chords) => chords.map(chordPitches));
      let track: Id;
      let createdTrack: string | null = null;
      if (target.type === "track") {
        const mine = w.track(w.project, target.id);
        if (!mine.instrument) throw fail("PT-176", { name: mine.name });
        track = mine.id;
      } else {
        const kind = w.catalog.instruments.find((k) => k.kind === target.kind);
        if (!kind) throw fail("TR-002", { kind: target.kind });
        createdTrack = kind.label;
        track = h()["tracks.add"]!({ name: kind.label, kind: kind.kind, defaultPitch: C3 }) as Id;
      }
      const start = barTick(w.project.tempoMap, startBar);
      const pass = bars.length * BAR_TICKS;
      const length = pass * repeat;
      const busy = w.project.clips.filter((c) => c.track === track && c.startTick < start + length && start < c.startTick + c.lengthTicks);
      if (busy.length > 0 && !replace) {
        throw fail("PT-370", { name: w.track(w.project, track).name, from: startBar, to: barIndex(w.project.tempoMap, start + length - 1) });
      }
      for (const clip of busy) h()["clips.remove"]!({ id: clip.id });
      const notes = passNotes(pitches, STRUM_GRIDS[strum]);
      const clip = h()["clips.addNote"]!({ track, startTick: start, lengthTicks: length, notes, loopTicks: pass }) as { id: Id };
      return { track, createdTrack, clip: clip.id, bars: bars.length * repeat };
    },
  };
}
