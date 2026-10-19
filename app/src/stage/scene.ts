// What the stage pictures (the room from above, the side view) share: the room's shape and how it is heard, who stands
// on the stage and where, and how a picture moves a channel. `StageView` makes it; the pictures only draw it and report the mouse.
import type { Accessor } from "solid-js";
import type { Id, Listening } from "../api/types";
import type { RoomShape, Spot } from "./stageRules";

export interface StageScene {
  shape: Accessor<RoomShape>;
  /** What the stage is heard on: on speakers a place behind the listener sounds like its mirror ahead. */
  listening: Accessor<Listening>;
  /** The channels on the stage, by node id (ids, so a dot's element lives on while its place changes). */
  placed: Accessor<Id[]>;
  /** Where a channel stands now: the dragged place while it is being dragged. */
  spotOf(node: Id): Spot | undefined;
  nameOf(node: Id): string;
  colorOf(node: Id): string;
  selected: Accessor<Id | null>;
  select(node: Id): void;
  /** A live drag: the dot follows the mouse, and the channel goes there. */
  drag(node: Id, spot: Spot): void;
  endDrag(): void;
  /** Puts a channel at a spot without a drag (a drop from the list). */
  place(node: Id, spot: Spot): void;
  /** Moves a channel on the stage a step from where it stands (the wheel, a key, a double-click). */
  nudge(node: Id, change: (spot: Spot) => Partial<Spot>): void;
}

/** How much a key or a wheel notch moves a dot. */
export const KEY_STEP = 0.02;
export const HEIGHT_STEP = 0.05;
