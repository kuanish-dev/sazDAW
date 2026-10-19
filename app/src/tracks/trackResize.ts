// Resizing a track's row by its header's bottom edge: while dragging, the height follows the pointer on screen
// (`tracks.resize`) and is saved on release (`tracks.setHeight`, one undo step); a double-click returns the track to the
// interface setting (Appearance → track height); with the edge focused, ↑/↓ change the height by a step.
import type { TracksActions } from "./tracks";
import type { Track } from "../api/types";
import { clamp } from "../common/math";
import { MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT, trackHeight } from "./rowLayout";
import { capture } from "../common/pointer";

/** One ↑/↓ press, pixels. */
const KEY_STEP = 8;

const fit = (height: number) => Math.round(clamp(height, MIN_TRACK_HEIGHT, MAX_TRACK_HEIGHT));

export function createTrackResize(deps: { track: () => Track; fallback: () => number; actions: Pick<TracksActions, "resize" | "setHeight"> }) {
  /** The drag: where the pointer started, the track's own height before it (`null` — the setting's) and the height now. */
  let drag: { startY: number; before: number | null; from: number; height: number } | null = null;
  const id = () => deps.track().id;
  const height = () => trackHeight(deps.track(), deps.fallback());

  return {
    onPointerDown(event: PointerEvent) {
      capture(event.currentTarget as Element, event);
      drag = { startY: event.clientY, before: deps.track().height ?? null, from: height(), height: height() };
    },
    onPointerMove(event: PointerEvent) {
      if (!drag) return;
      drag.height = fit(drag.from + event.clientY - drag.startY);
      deps.actions.resize(id(), drag.height);
    },
    onPointerUp() {
      if (!drag) return;
      const { from, height } = drag;
      drag = null;
      if (height !== from) void deps.actions.setHeight(id(), height);
    },
    onPointerCancel() {
      if (!drag) return;
      deps.actions.resize(id(), drag.before);
      drag = null;
    },
    onDblClick() {
      void deps.actions.setHeight(id(), null);
    },
    onKeyDown(event: KeyboardEvent) {
      if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
      event.preventDefault();
      void deps.actions.setHeight(id(), fit(height() + (event.key === "ArrowUp" ? -KEY_STEP : KEY_STEP)));
    },
  };
}
