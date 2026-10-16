// Mouse on the sample waveform: drag and edge selection, ruler markers (add, move, remove), wheel zoom and scroll.
import {
  addMarker, frameAtX, moveMarker, nearestMarker, normalizeSelection, panBy, removeMarker, segmentAt, zoomAt, type Selection, type View,
} from "../../domain/waveform";
import { clamp } from "../../domain/math";
import { capture, localPoint } from "../common/pointer";
import { RULER_HEIGHT } from "./waveformRender";

/** How many pixels from a marker or selection edge it can be grabbed. */
const GRAB_PX = 7;
/** Movement below this (pixels) is a click (place the cursor), not a selection. */
const CLICK_PX = 3;
/** Zoom factor per wheel notch. */
const WHEEL_ZOOM = 1.2;

type Drag =
  | { mode: "select"; anchor: number; moved: boolean }
  | { mode: "edge"; fixed: number }
  | { mode: "marker"; original: number | null; current: number };

export interface WaveformPointerDeps {
  canvas(): HTMLCanvasElement;
  width(): number;
  total(): number;
  view(): View;
  selection(): Selection | null;
  markers(): number[];
  setHover(frame: number | null): void;
  /** Markers while dragging (until mouse release); `null` means no draft. */
  setDraftMarkers(markers: number[] | null): void;
  draftMarkers(): number[] | null;
  onView(view: View): void;
  onSelection(selection: Selection | null): void;
  onCursor(frame: number): void;
  onMarkers(markers: number[]): void;
}

export function createWaveformPointer(deps: WaveformPointerDeps) {
  let drag: Drag | null = null;

  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.canvas(), event);
    return { x, onRuler: y < RULER_HEIGHT, frame: Math.round(clamp(frameAtX(x, deps.width(), deps.view()), 0, deps.total())) };
  };
  const grabFrames = (px = GRAB_PX) => (px / deps.width()) * deps.view().span;

  const nearEdge = (frame: number): "from" | "to" | null => {
    const sel = deps.selection();
    if (!sel) return null;
    if (Math.abs(frame - sel.from) <= grabFrames()) return "from";
    if (Math.abs(frame - sel.to) <= grabFrames()) return "to";
    return null;
  };

  const onDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    capture(deps.canvas(), event);
    const { frame, onRuler } = locate(event);
    if (onRuler) {
      // On the ruler: grab an existing marker or place a new one.
      const existing = nearestMarker(deps.markers(), frame, grabFrames());
      if (existing === null) deps.setDraftMarkers(addMarker(deps.markers(), frame, deps.total()));
      drag = { mode: "marker", original: existing, current: existing ?? frame };
      return;
    }
    const edge = nearEdge(frame);
    const sel = deps.selection();
    drag = edge && sel ? { mode: "edge", fixed: edge === "from" ? sel.to : sel.from } : { mode: "select", anchor: frame, moved: false };
  };

  const hoverCursor = (frame: number, onRuler: boolean) => {
    const overMarker = onRuler && nearestMarker(deps.markers(), frame, grabFrames()) !== null;
    const overEdge = !onRuler && nearEdge(frame) !== null;
    deps.canvas().style.cursor = overMarker || overEdge ? "ew-resize" : onRuler ? "copy" : "crosshair";
  };

  const onMove = (event: PointerEvent) => {
    const { frame, onRuler } = locate(event);
    deps.setHover(onRuler ? null : frame);
    if (!drag) return hoverCursor(frame, onRuler);
    if (drag.mode === "select") {
      if (!drag.moved && Math.abs(frame - drag.anchor) < grabFrames(CLICK_PX)) return;
      drag.moved = true;
      deps.onSelection(normalizeSelection(drag.anchor, frame, deps.total()));
    } else if (drag.mode === "edge") {
      deps.onSelection(normalizeSelection(drag.fixed, frame, deps.total()));
    } else {
      const next = drag.original === null
        ? addMarker(removeMarker(deps.markers(), drag.current), frame, deps.total())
        : moveMarker(deps.markers(), drag.original, frame, deps.total());
      drag.current = frame;
      deps.setDraftMarkers(next);
    }
  };

  const onUp = (event: PointerEvent) => {
    const finished = drag;
    drag = null;
    if (finished?.mode === "select" && !finished.moved) {
      deps.onCursor(locate(event).frame);
      deps.onSelection(null);
    } else if (finished?.mode === "marker") {
      const next = deps.draftMarkers();
      deps.setDraftMarkers(null);
      if (next) deps.onMarkers(next);
    }
  };

  /** Double click on the waveform selects the span between markers (the whole file if there are none). */
  const onDouble = (event: MouseEvent) => {
    const { frame, onRuler } = locate(event);
    if (!onRuler) deps.onSelection(segmentAt(deps.markers(), frame, deps.total()) ?? { from: 0, to: deps.total() });
  };

  /** Right click: on the ruler removes a marker, on the waveform clears the selection. */
  const onContext = (event: MouseEvent) => {
    event.preventDefault();
    const { frame, onRuler } = locate(event);
    if (!onRuler) return deps.onSelection(null);
    const near = nearestMarker(deps.markers(), frame, grabFrames());
    if (near !== null) deps.onMarkers(removeMarker(deps.markers(), near));
  };

  /** Wheel zooms around the cursor; Shift + wheel or horizontal scroll pans. */
  const onWheel = (event: WheelEvent) => {
    event.preventDefault();
    const view = deps.view();
    if (event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
      const delta = (event.shiftKey ? event.deltaY : event.deltaX) || 0;
      return deps.onView(panBy(view, (delta / deps.width()) * view.span, deps.total()));
    }
    const factor = event.deltaY < 0 ? WHEEL_ZOOM : 1 / WHEEL_ZOOM;
    deps.onView(zoomAt(view, frameAtX(localPoint(deps.canvas(), event).x, deps.width(), view), factor, deps.total()));
  };

  return { onDown, onMove, onUp, onDouble, onContext, onWheel, onLeave: () => deps.setHover(null) };
}
