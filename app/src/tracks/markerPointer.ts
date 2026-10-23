import { createSignal } from "solid-js";
import type { Id, Marker, TempoMap } from "../api/types";
import type { Markers } from "./markers";
import { barStart, snapNearestInBars } from "../common/tempoMap";
import { BAR_GRID } from "../common/time";
import { capture, localPoint } from "../common/pointer";
import { markerAt } from "./markerLayout";
import { RULER_HEIGHT, tickAtX } from "./timelineLayout";

/** Mouse movement below this (pixels) is a click, not a drag. */
const DRAG_MIN_PX = 3;

interface Drag {
  marker: Marker;
  /** Where the flag was grabbed relative to the marker start, in ticks. */
  grab: number;
  startX: number;
  moved: boolean;
}

export function createMarkerPointer(deps: {
  surface: () => HTMLElement;
  markers: () => readonly Marker[];
  pxPerTick: () => number;
  /** Tempo map: markers snap to the starts of its bars. */
  tempoMap: () => TempoMap;
  actions: Markers;
  seek: (tick: number) => void;
}) {
  /** Move draft: the marker and its new position while dragging. */
  const [draft, setDraft] = createSignal<{ id: Id; tick: number } | null>(null);
  /** Marker whose name is being edited on the ruler. */
  const [renaming, setRenaming] = createSignal<Marker | null>(null);
  let drag: Drag | null = null;

  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.surface(), event);
    return { x, y, tick: tickAtX(x, deps.pxPerTick()), hit: y < RULER_HEIGHT ? markerAt(deps.markers(), x, y, deps.pxPerTick()) : null };
  };

  return {
    draft,
    renaming,
    stopRenaming: () => setRenaming(null),
    onDown(event: PointerEvent): boolean {
      if (event.button !== 0) return false;
      const { x, tick, hit } = locate(event);
      if (!hit) return false;
      capture(deps.surface(), event);
      drag = { marker: hit, grab: tick - hit.tick, startX: x, moved: false };
      return true;
    },
    onMove(event: PointerEvent): boolean {
      if (!drag) return false;
      const { x, tick } = locate(event);
      if (!drag.moved && Math.abs(x - drag.startX) < DRAG_MIN_PX) return true;
      drag.moved = true;
      setDraft({ id: drag.marker.id, tick: Math.max(0, snapNearestInBars(deps.tempoMap(), tick - drag.grab, BAR_GRID)) });
      return true;
    },
    onUp(): boolean {
      if (!drag) return false;
      const { marker, moved } = drag;
      const moveTo = draft();
      drag = null;
      setDraft(null);
      if (!moved) deps.seek(marker.tick);
      else if (moveTo && moveTo.tick !== marker.tick) void deps.actions.move(marker.id, moveTo.tick);
      return true;
    },
    onDouble(event: MouseEvent): boolean {
      const { y, tick, hit } = locate(event);
      if (y >= RULER_HEIGHT) return false;
      if (hit) setRenaming(hit);
      else if (event.altKey) void deps.actions.add(barStart(deps.tempoMap(), tick));
      else return false;
      return true;
    },
    onContext(event: MouseEvent): boolean {
      const { hit } = locate(event);
      if (!hit) return false;
      event.preventDefault();
      void deps.actions.remove(hit.id);
      return true;
    },
  };
}
