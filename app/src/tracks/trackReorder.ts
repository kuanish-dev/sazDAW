// Reordering tracks by dragging the gripper handle: computes the gap between rows (tracks and folders) and hands it to the caller
// on release; what it means (into a folder, out of one, just up or down) is decided by `tracks/trackRows.ts: planTrackDrop`.
import { createSignal } from "solid-js";
import type { Id } from "../api/types";
import { type RowLayout, insertSlot, rowTop } from "./rowLayout";
import { capture } from "../common/pointer";

interface Drag {
  id: Id;
  from: number;
  /** Header row: the list top is measured from it (so page scrolling during the drag is accounted for). */
  row: HTMLElement;
}

export function createTrackReorder(deps: { layout: () => RowLayout; drop: (id: Id, slot: number) => void }) {
  const [slot, setSlot] = createSignal<number | null>(null);
  let drag: Drag | null = null;

  const slotAt = (event: PointerEvent) => {
    const top = drag!.row.getBoundingClientRect().top - rowTop(drag!.from, deps.layout());
    return insertSlot(event.clientY - top, deps.layout());
  };

  return {
    /** Insertion line between rows: gap index, or `null` when not dragging. */
    slot,
    onDown(event: PointerEvent, id: Id, from: number) {
      const handle = event.currentTarget as HTMLElement;
      const row = handle.closest<HTMLElement>("[data-track-row]");
      if (!row) return;
      capture(handle, event);
      drag = { id, from, row };
      setSlot(slotAt(event));
    },
    onMove(event: PointerEvent) {
      if (drag) setSlot(slotAt(event));
    },
    onUp(event: PointerEvent) {
      if (!drag) return;
      const slot = slotAt(event);
      const id = drag.id;
      drag = null;
      setSlot(null);
      deps.drop(id, slot);
    },
    onCancel() {
      drag = null;
      setSlot(null);
    },
  };
}
