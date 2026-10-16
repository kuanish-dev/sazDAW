// Mouse in the node editor: camera pan and zoom, moving cards, dragging a wire from an output to an input.
// Holds the gesture and exposes handlers; the result goes to `GraphActions`, drafts are shown while the gesture is in progress.
import type { GraphActions } from "../../actions/graph";
import type { Graph, GraphNode, Id } from "../../api/types";
import { checkEdge } from "../../domain/graph";
import { capture, isZoomWheel, localPoint } from "../common/pointer";
import { hitInputPort, outPort, toWorld, zoomAt, type Point, type View } from "./graphLayout";

/** Zoom sensitivity to the wheel (trackpad pinch). */
const WHEEL_ZOOM_RATE = 0.01;

/** Wire being dragged: its source, the pointer position and the input it is over (with the rejection reason). */
export interface Wire {
  from: Id;
  at: Point;
  target: { id: Id; sidechain: boolean; error: string | null } | null;
}

type Gesture =
  | { kind: "pan"; startX: number; startY: number; view: View }
  | { kind: "node"; id: Id; grab: Point; moved: boolean }
  | { kind: "wire"; from: Id };

export interface GraphPointerDeps {
  graph: GraphActions;
  data(): Graph;
  /** Nodes with position drafts applied. */
  nodes(): GraphNode[];
  container(): HTMLElement;
  view(): View;
  setView(view: View): void;
  setDrafts(drafts: Record<Id, Point>): void;
  setWire(wire: Wire | null): void;
  /** Selects a node (`null` clears the selection). */
  select(id: Id | null): void;
}

export function createGraphPointer(deps: GraphPointerDeps) {
  let gesture: Gesture | null = null;

  const worldOf = (event: { clientX: number; clientY: number }) => toWorld(deps.view(), localPoint(deps.container(), event));
  const grabTo = (event: PointerEvent, grab: Point) => {
    const at = worldOf(event);
    return { x: Math.round(at.x - grab.x), y: Math.round(at.y - grab.y) };
  };

  const moveWire = (from: Id, event: PointerEvent) => {
    const at = worldOf(event);
    const hit = hitInputPort(deps.nodes(), at);
    const target = hit && { ...hit, error: checkEdge(deps.data(), from, hit.id, hit.sidechain) };
    deps.setWire({ from, at, target });
  };

  return {
    onBackgroundDown(event: PointerEvent) {
      deps.select(null);
      capture(deps.container(), event);
      gesture = { kind: "pan", startX: event.clientX, startY: event.clientY, view: deps.view() };
    },

    onNodeDown(event: PointerEvent, node: GraphNode) {
      const at = worldOf(event);
      capture(deps.container(), event);
      gesture = { kind: "node", id: node.id, grab: { x: at.x - node.x, y: at.y - node.y }, moved: false };
    },

    onOutputDown(event: PointerEvent, node: GraphNode) {
      event.stopPropagation();
      deps.select(node.id);
      capture(deps.container(), event);
      gesture = { kind: "wire", from: node.id };
      deps.setWire({ from: node.id, at: outPort(node), target: null });
    },

    onMove(event: PointerEvent) {
      if (!gesture) return;
      if (gesture.kind === "pan") {
        const { view } = gesture;
        deps.setView({ ...view, x: view.x + event.clientX - gesture.startX, y: view.y + event.clientY - gesture.startY });
      } else if (gesture.kind === "node") {
        gesture.moved = true;
        deps.setDrafts({ [gesture.id]: grabTo(event, gesture.grab) });
      } else {
        moveWire(gesture.from, event);
      }
    },

    onUp(event: PointerEvent) {
      const done = gesture;
      gesture = null;
      if (done?.kind === "node" && done.moved) {
        const { x, y } = grabTo(event, done.grab);
        // The draft stays until the backend answers so the card doesn't jump back.
        void deps.graph.moveNode(done.id, x, y).finally(() => deps.setDrafts({}));
      } else if (done?.kind === "wire") {
        const hit = hitInputPort(deps.nodes(), worldOf(event));
        deps.setWire(null);
        if (hit && checkEdge(deps.data(), done.from, hit.id, hit.sidechain) === null) {
          void deps.graph.connect(done.from, hit.id, 1, hit.sidechain);
        }
      }
    },

    onWheel(event: WheelEvent) {
      // The wheel over a knob or fader turns it instead of panning the canvas.
      if (!isZoomWheel(event) && (event.target as HTMLElement).closest("[role=slider]")) return;
      event.preventDefault();
      const view = deps.view();
      if (isZoomWheel(event)) deps.setView(zoomAt(view, localPoint(deps.container(), event), Math.exp(-event.deltaY * WHEEL_ZOOM_RATE)));
      else deps.setView({ ...view, x: view.x - event.deltaX, y: view.y - event.deltaY });
    },
  };
}

export type GraphPointer = ReturnType<typeof createGraphPointer>;
