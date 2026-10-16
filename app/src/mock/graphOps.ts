// Shared graph edits inside a project: the life cycle of source nodes and connection-level lanes (like `ops.rs` in the backend).
import type { GraphEdge, GraphNode, Id, NodeKind, Project } from "../api/types";
import { MASTER_NODE } from "../api/types";
import { acceptsInput, hasOutput, MAX_EDGES, MAX_NODES, reaches } from "../domain/graph";
import { fail } from "./errors";

export function limitNodes(p: Project) {
  if (p.graph.nodes.length >= MAX_NODES) throw fail("GR-210", { max: MAX_NODES });
}

export function limitEdges(p: Project) {
  if (p.graph.edges.length >= MAX_EDGES) throw fail("GR-211", { max: MAX_EDGES });
}

export function nodeOf(p: Project, id: Id): GraphNode {
  const node = p.graph.nodes.find((n) => n.id === id);
  if (!node) throw fail("GR-001", { id });
  return node;
}

/** The connection `from → to` is allowed (as the backend's graph rules), otherwise its error is thrown. */
export function checkEdge(p: Project, from: Id, to: Id, sidechain: boolean) {
  const source = nodeOf(p, from);
  const target = nodeOf(p, to);
  if (from === to) throw fail("GR-101");
  if (!hasOutput(source.kind)) throw fail("GR-102");
  if (!acceptsInput(target.kind)) throw fail("GR-103");
  if (sidechain && target.kind.type !== "effect") throw fail("GR-104");
  if (reaches(p.graph, to, from)) throw fail("GR-105");
}

/** A source node connected to the master: a track creates one along with itself. */
export function addSource(p: Project, name: string, kind: NodeKind, x: number, y: number) {
  limitNodes(p);
  limitEdges(p);
  const id = p.nextId++;
  p.graph.nodes.push({ id, name, kind, x, y });
  p.graph.edges.push({ from: id, to: MASTER_NODE, level: 1, sidechain: false, preFader: false });
}

/** Removes a node together with its connections and the automation lanes that target it. */
export function dropNode(p: Project, id: Id) {
  p.graph.nodes = p.graph.nodes.filter((n) => n.id !== id);
  p.graph.edges = p.graph.edges.filter((e) => e.from !== id && e.to !== id);
  p.automation = p.automation.filter((l) => {
    const t = l.target;
    return t.type === "instrumentParam" || t.type === "edgeLevel" || t.node !== id;
  });
  dropStaleEdgeLanes(p);
}

/** Removes connection-level lanes whose connection is gone (a lane lives as long as its connection). */
export function dropStaleEdgeLanes(p: Project) {
  p.automation = p.automation.filter(({ target: t }) =>
    t.type !== "edgeLevel" || p.graph.edges.some((e) => e.from === t.from && e.to === t.to && e.sidechain === t.sidechain));
}

/** A connection-level lane follows its connection when inserting an effect moves it to another node. */
export function moveEdgeLane(p: Project, old: GraphEdge, from: Id, to: Id) {
  for (const lane of p.automation) {
    const t = lane.target;
    if (t.type === "edgeLevel" && t.from === old.from && t.to === old.to && t.sidechain === old.sidechain) {
      lane.target = { ...t, from, to };
    }
  }
}
