// Mouse on the controller lane: dragging draws points on grid steps (line points in the dragged span are replaced
// by the drawn ones); right click removes the nearest point. During the gesture points are a draft; release makes one edit.
import type { ControllerKind, ControllerPoint } from "../../api/types";
import { extendStroke, fromFraction, removeNear, startStroke, strokeOver, type PencilStroke } from "../../domain/controllers";
import { capture, localPoint } from "../common/pointer";
import { fractionAtY, tickAtX } from "./pianoRollLayout";

/** How far (pixels) from a point a right click still finds it. */
const GRAB_PX = 8;

export interface ControllerPointerDeps {
  canvas(): HTMLCanvasElement;
  kind(): ControllerKind;
  /** Line points in the project (without the draft). */
  points(): ControllerPoint[];
  pxPerTick(): number;
  /** Point step while drawing: the panel grid. */
  step(): number;
  /** First tick where a point can no longer be placed: the end of the visible area. */
  endTick(): number;
  setDraft(points: ControllerPoint[] | null): void;
  commit(points: ControllerPoint[]): Promise<void>;
}

export function createControllerPointer(deps: ControllerPointerDeps) {
  /** The drawing gesture; `null` — none. */
  let stroke: PencilStroke | null = null;

  const locate = (event: PointerEvent): ControllerPoint => {
    const { x, y } = localPoint(deps.canvas(), event);
    const step = deps.step();
    const lastTick = Math.floor((deps.endTick() - 1) / step) * step;
    const tick = Math.min(Math.floor(tickAtX(x, deps.pxPerTick()) / step) * step, lastTick);
    return { tick: Math.max(0, tick), value: fromFraction(deps.kind(), fractionAtY(y)) };
  };

  const result = () => (stroke ? strokeOver(deps.points(), stroke) : deps.points());

  return {
    onDown(event: PointerEvent) {
      if (event.button === 2) {
        // Search at the exact mouse position, not the grid: the point may have been placed with a different grid.
        const tick = tickAtX(localPoint(deps.canvas(), event).x, deps.pxPerTick());
        const left = removeNear(deps.points(), tick, GRAB_PX / deps.pxPerTick());
        if (left !== deps.points()) void deps.commit(left);
        return;
      }
      if (event.button !== 0) return;
      const at = locate(event);
      capture(deps.canvas(), event);
      stroke = startStroke(at);
      deps.setDraft(result());
    },
    onMove(event: PointerEvent) {
      if (!stroke) return;
      stroke = extendStroke(stroke, locate(event), deps.step());
      deps.setDraft(result());
    },
    async onUp() {
      if (!stroke) return;
      const points = result();
      stroke = null;
      await deps.commit(points);
      deps.setDraft(null);
    },
  };
}
