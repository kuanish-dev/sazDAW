// The signal path through a node, for the path view on the Graph page (`ui/graph/SignalPath.tsx`): where the node's
// sound comes from, back while it has a single regular input, and where it goes, on along each node's main output (the
// mixer's convention, `mixerStrips.ts: mainOutput`) to the master. Sends, sidechain keys and the other inputs of a
// merging bus branch off this one way and are left out.
import type { Graph, GraphNode, Id } from "../api/types";
import { findNode, incoming } from "./graph";
import { mainOutput } from "./mixerStrips";

/** The nodes along the path through `id`, source first; empty for an unknown node. */
export function signalPath(graph: Graph, id: Id): GraphNode[] {
  const start = findNode(graph, id);
  if (!start) return [];
  const path = [start];
  const seen = new Set([id]);
  // Bounded by the node count: the graph is acyclic, but the guards keep bad data from hanging the view.
  let head = start;
  for (let i = 0; i < graph.nodes.length; i++) {
    const inputs = incoming(graph, head.id).filter((e) => !e.sidechain);
    const before = inputs.length === 1 ? findNode(graph, inputs[0]!.from) : undefined;
    if (!before || seen.has(before.id)) break;
    path.unshift(before);
    seen.add(before.id);
    head = before;
  }
  let tail = start;
  for (let i = 0; i < graph.nodes.length; i++) {
    const edge = mainOutput(graph, tail.id);
    const next = edge && findNode(graph, edge.to);
    if (!next || seen.has(next.id)) break;
    path.push(next);
    seen.add(next.id);
    tail = next;
  }
  return path;
}
