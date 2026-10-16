// Chord parts: strum patterns and the request to strum a chord progression on one track.
import type { Id } from "./model";

/** A strum: one 4/4 bar of eighth notes, `D` down, `U` up, `X` a muted down stroke ("chuck"), lower case softer, `.` nothing. */
export interface StrumPattern {
  id: string;
  name: string;
  grid: string;
}

/** Where the part plays: an existing instrument track or a new track of an instrument kind. */
export type ChordTarget = { type: "track"; id: Id } | { type: "new"; kind: string };

export interface ChordPartRequest {
  target: ChordTarget;
  /** Bar (zero-based) where the part starts. */
  startBar: number;
  /** Strum id: `four`, `six`, `eight`. */
  strum: string;
  /** One entry per bar: one chord symbol ("Am", "F#m7"…) or two (the second from the middle of the bar). */
  bars: string[][];
  /** How many times the progression plays; it loops inside one clip. */
  repeat: number;
  /** `true`: clips already occupying the space are removed; `false`: rejected with a readable message. */
  replace: boolean;
}

export interface ChordPartReport {
  track: Id;
  /** The new track's name when the target was `new`. */
  createdTrack: string | null;
  clip: Id;
  bars: number;
}
