import type { Graph, GraphNode, Id } from "../api/types";
import { findNode, incoming } from "./graphRules";
import { mainOutput } from "../mixer/mixerStrips";

/** The nodes along the path through `id`, source first; empty for an unknown node. */
export function signalPath(graph: Graph, id: Id): GraphNode[] {
  const start = findNode(graph, id);
  if (!start) return [];
  const path = [start];
  const seen = new Set([id]);
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
