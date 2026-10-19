// What is selected in the node editor (a node or an edge) and how to remove it.
import { createSignal } from "solid-js";
import type { ProcessingGraph } from "./graph";
import type { GraphEdge, GraphNode, Id } from "../api/types";

export const sameEdge = (a: GraphEdge, b: GraphEdge) => a.from === b.from && a.to === b.to && a.sidechain === b.sidechain;

/** Only effects and buses can be removed (sources go away with their track, the master is permanent). */
export const isRemovable = (node: GraphNode | null) => node?.kind.type === "effect" || node?.kind.type === "bus";

export function createGraphSelection(deps: { graph: ProcessingGraph; nodes(): GraphNode[]; edges(): GraphEdge[] }) {
  const [nodeId, setNodeId] = createSignal<Id | null>(null);
  const [edgeKey, setEdgeKey] = createSignal<GraphEdge | null>(null);

  /** Selected node (with position drafts); `null` if it's gone. */
  const node = () => {
    const id = nodeId();
    return id === null ? null : deps.nodes().find((n) => n.id === id) ?? null;
  };
  /** Selected edge in its current form (its level may have changed); `null` if it's gone. */
  const edge = () => {
    const key = edgeKey();
    return key ? deps.edges().find((e) => sameEdge(e, key)) ?? null : null;
  };

  return {
    nodeId,
    node,
    edge,
    /** Selects a node (or clears the selection); the edge selection is reset. */
    selectNode(id: Id | null) {
      setNodeId(id);
      setEdgeKey(null);
    },
    selectEdge(e: GraphEdge) {
      setEdgeKey(e);
      setNodeId(null);
    },
    /** There is something to remove with the Delete button or key. */
    canRemove: () => edge() !== null || isRemovable(node()),
    /** Removes the selected edge, or if there is none, the selected effect or bus. */
    removeSelection() {
      const e = edge();
      const n = node();
      if (e) void deps.graph.disconnect(e.from, e.to, e.sidechain);
      else if (n && isRemovable(n)) void deps.graph.removeNode(n.id);
      setEdgeKey(null);
      if (!e) setNodeId(null);
    },
  };
}

export type GraphSelection = ReturnType<typeof createGraphSelection>;
