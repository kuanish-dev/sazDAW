// Drum patterns and the request to build a drum part from them.
import type { Id } from "./model";

/** What the drum pattern is for in a song. */
export type PatternRole = "groove" | "fill" | "break" | "intro" | "ending";

/** One drum of a pattern: a 16th-note grid, `X` accent, `x` normal, `o` ghost, `.` rest; bars separated by `|`. */
export interface PatternLane {
  lane: string;
  grid: string;
}

export interface DrumPattern {
  id: string;
  name: string;
  /** Genre or group: `rock`, `house`, `fills`… */
  style: string;
  role: PatternRole;
  bars: number;
  description: string;
  lanes: PatternLane[];
}

/** A step of the part: a pattern and how many times in a row to play it. */
export interface PatternSlot {
  pattern: string;
  repeat: number;
}

/** What plays a pattern drum. */
export type LaneTarget =
  /** The track with this built-in drum if it exists; otherwise one is created. */
  | { type: "auto" }
  /** The drum is not played. */
  | { type: "skip" }
  /** Your track; `pitch` is the note pitch (defaults to the track's step-input pitch). */
  | { type: "track"; id: Id; pitch?: number | null }
  /** Your project sample: a sampler track is created for the drum. */
  | { type: "sample"; id: Id };

export interface LaneAssignment {
  lane: string;
  target: LaneTarget;
}

export interface DrumPartRequest {
  /** Bar (zero-based) where the part starts. */
  startBar: number;
  slots: PatternSlot[];
  /** `true`: clips already occupying the space are removed; `false`: rejected with a readable message. */
  replace: boolean;
  /** What plays each drum; drums not listed here are `auto`. */
  lanes: LaneAssignment[];
}

export interface DrumPartReport {
  bars: number;
  clips: Id[];
  tracks: Id[];
  /** Names of tracks that did not exist and were created. */
  createdTracks: string[];
}
