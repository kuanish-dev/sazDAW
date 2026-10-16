// Pointer work shared by the stage pictures: from the mouse to the picture's own units (metres of the room), dragging
// a dot with the grab point kept under the mouse, and the off-stage chips dropped into the room.
import type { Id } from "../../api/types";
import type { LocalPoint } from "../common/pointer";

/** The data type of an off-stage channel dragged from the list into the room. */
export const STAGE_CHANNEL_MIME = "application/x-daw-stage-channel";

/** The event's point in the SVG's own (viewBox) units. */
export function svgPoint(svg: SVGSVGElement, event: { clientX: number; clientY: number }): LocalPoint {
  const matrix = svg.getScreenCTM();
  if (!matrix) return { x: 0, y: 0 };
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse());
  return { x: point.x, y: point.y };
}

/**
 * Drags a dot: `at` is the dot's own point in SVG units when the drag starts; every move reports where the dot is now
 * (the grab offset is kept, so the dot does not jump under the mouse), and `onEnd` comes on release.
 */
export function dragDot(event: PointerEvent, svg: SVGSVGElement, at: LocalPoint, onMove: (point: LocalPoint) => void, onEnd: () => void) {
  const target = event.currentTarget as Element;
  const start = svgPoint(svg, event);
  const grab = { x: start.x - at.x, y: start.y - at.y };
  target.setPointerCapture(event.pointerId);
  const move = (e: PointerEvent) => {
    const point = svgPoint(svg, e);
    onMove({ x: point.x - grab.x, y: point.y - grab.y });
  };
  const end = () => {
    target.removeEventListener("pointermove", move as EventListener);
    target.removeEventListener("pointerup", end);
    target.removeEventListener("pointercancel", end);
    onEnd();
  };
  target.addEventListener("pointermove", move as EventListener);
  target.addEventListener("pointerup", end);
  target.addEventListener("pointercancel", end);
}

/** The off-stage channel being dropped, if the drag carries one. */
export function droppedChannel(event: DragEvent): Id | null {
  const raw = event.dataTransfer?.getData(STAGE_CHANNEL_MIME);
  const node = raw ? Number(raw) : Number.NaN;
  return Number.isInteger(node) ? node : null;
}

export const carriesChannel = (event: DragEvent): boolean => event.dataTransfer?.types.includes(STAGE_CHANNEL_MIME) ?? false;
