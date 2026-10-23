import type { NewNote, Note } from "../api/types";
import { stripId } from "./noteFit";
import type { NoteChange } from "./noteSelection";
import { capture, localPoint } from "../common/pointer";
import { tickAtX, velocityAtY } from "./pianoRollLayout";

/** How far (pixels) from a note's start its bar can still be grabbed. */
const GRAB_PX = 10;

export interface VelocityPointerDeps {
  velocityCanvas(): HTMLCanvasElement;
  noteList(): Note[];
  selected(): Set<number>;
  selectedNotes(): Note[];
  pxPerTick(): number;
  setDrafts(drafts: Record<number, NewNote>): void;
  commitDrafts(group: Note[]): Promise<NoteChange[]>;
}

export function createVelocityPointer(deps: VelocityPointerDeps) {
  let group: Note[] | null = null;

  const locate = (event: PointerEvent) => {
    const { x, y } = localPoint(deps.velocityCanvas(), event);
    return { tick: tickAtX(x, deps.pxPerTick()), velocity: velocityAtY(y) };
  };
  const draftVelocity = (notes: Note[], velocity: number) =>
    deps.setDrafts(Object.fromEntries(notes.map((n) => [n.id, { ...stripId(n), velocity }])));

  return {
    onDown(event: PointerEvent) {
      const { tick, velocity } = locate(event);
      const distance = (n: Note) => Math.abs(n.startTick - tick);
      const near = [...deps.noteList()].sort((a, b) => distance(a) - distance(b))[0];
      if (!near || distance(near) * deps.pxPerTick() > GRAB_PX) return;
      capture(deps.velocityCanvas(), event);
      group = deps.selected().has(near.id) ? deps.selectedNotes() : [near];
      draftVelocity(group, velocity);
    },
    onMove(event: PointerEvent) {
      if (group) draftVelocity(group, locate(event).velocity);
    },
    async onUp() {
      const finished = group;
      group = null;
      if (finished) await deps.commitDrafts(finished);
    },
  };
}
