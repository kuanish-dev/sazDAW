// The virtual stage (`StageControl`): where a mixer channel stands in the shared room, and the room itself.

/** Where a channel stands on the stage besides its left/right, which is the strip's `pan` (−1 left wall … 1 right wall).
 *  The listener stands in the middle of the room facing ahead: all three coordinates are 0 at their head. */
export interface StagePlace {
  /** −1 the wall behind the listener … 0 level with their ears … 1 the wall ahead. */
  depth: number;
  /** −1 the floor … 0 the listener's ear height … 1 the ceiling. */
  height: number;
}

/** What the stage is rendered for: equal-power panning on speakers (a place behind sounds like its mirror ahead), binaural
 *  cues all around the head on headphones (behind and above included). */
export type Listening = "speakers" | "headphones";

/** The shared room every channel on the stage plays in. */
export interface Room {
  /** 0 a small studio room (6 × 5 × 3 m) … 1 a concert hall (30 × 24 × 10 m). */
  size: number;
  /** 0 no reflections (open air) … 1 hard, live walls. */
  reverb: number;
  listening: Listening;
}

/** How many channels can stand on the stage at once (`daw_contracts::MAX_ON_STAGE`). */
export const MAX_ON_STAGE = 64;

/** Where a channel first stands (`StagePlace::DEFAULT`): a few steps in front of the listener, at ear height. */
export const DEFAULT_STAGE_PLACE: StagePlace = { depth: 0.5, height: 0 };

/** A new project's room (`Room::default`). */
export const DEFAULT_ROOM: Room = { size: 0.4, reverb: 0.4, listening: "speakers" };
