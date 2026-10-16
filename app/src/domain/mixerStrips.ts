// Mixer strips from the processing graph: which nodes to show as strips (tracks in order, buses, master), which effects
// are a strip's inserts, where its main output goes and which edges are sends. The graph is free-form, so this is a
// display convention: the main path is the node's first regular post-fader edge (track creation, `setOutput` and
// effect insertion all keep edges in order), other regular edges are sends. A pre-fader edge is
// always a send: inserting an effect or changing the output leaves it on the node.
import { type Graph, type GraphEdge, type GraphNode, type Id, MASTER_NODE, type Project } from "../api/types";
import { checkEdge, findNode, hasStrip, incoming, nodeLabel, outgoing, sourceOfTrack } from "./graph";

export interface StripModel {
  node: GraphNode;
  /** Insert effects along the signal path (for master, the effects right before it). */
  inserts: GraphNode[];
  /** Node the main output and sends leave from: the last insert or the node itself. */
  tail: Id;
  /** Main output (`null` for master). */
  output: GraphEdge | null;
  /** Sends: the other regular edges of the node and its inserts. */
  sends: GraphEdge[];
}

const normalOut = (graph: Graph, id: Id) => outgoing(graph, id).filter((e) => !e.sidechain);

/** Regular edges of a node: the signal path (first post-fader edge) and sends (the rest). */
function splitOut(graph: Graph, id: Id): { path: GraphEdge | undefined; sends: GraphEdge[] } {
  const edges = normalOut(graph, id);
  const path = edges.find((e) => !e.preFader);
  return { path, sends: edges.filter((e) => e !== path) };
}
/** The node's main output, the next step of its signal path: its first regular post-fader edge. */
export const mainOutput = (graph: Graph, id: Id): GraphEdge | undefined => splitOut(graph, id).path;

const normalIn = (graph: Graph, id: Id) => incoming(graph, id).filter((e) => !e.sidechain);

/** An insert is an effect that receives audio only from this point of the path (otherwise it is a shared effect, not an insert). */
function insertAfter(graph: Graph, id: Id): GraphNode | undefined {
  const { path } = splitOut(graph, id);
  const next = path && findNode(graph, path.to);
  if (next?.kind.type !== "effect" || normalIn(graph, next.id).length !== 1) return undefined;
  return next;
}

/** Strip nodes left to right: track sources in track order, then buses by id. Master is separate. */
export function stripOrder(project: Project): Id[] {
  const sources = project.tracks.flatMap((t) => sourceOfTrack(project.graph, t.id)?.id ?? []);
  const buses = project.graph.nodes.filter((n) => n.kind.type === "bus").map((n) => n.id).sort((a, b) => a - b);
  return [...sources, ...buses];
}

/** Strip of a source or bus: inserts along the first edge, main output and sends. */
export function stripModel(graph: Graph, id: Id): StripModel | undefined {
  const node = findNode(graph, id);
  if (!node) return undefined;
  const inserts: GraphNode[] = [];
  const sends: GraphEdge[] = [];
  let tail = id;
  // Bounded by the node count: the graph is acyclic, but the counter guards against hanging on bad data.
  for (let i = 0; i < graph.nodes.length; i++) {
    const next = insertAfter(graph, tail);
    if (!next) break;
    sends.push(...splitOut(graph, tail).sends);
    inserts.push(next);
    tail = next.id;
  }
  const last = splitOut(graph, tail);
  return { node, inserts, tail, output: last.path ?? null, sends: [...sends, ...last.sends] };
}

/** Master strip: effects that all of its input passes through (excluding other strips' inserts). */
export function masterModel(graph: Graph): StripModel | undefined {
  const node = findNode(graph, MASTER_NODE);
  if (!node) return undefined;
  const taken = new Set(graph.nodes.filter((n) => hasStrip(n.kind)).flatMap((n) => stripModel(graph, n.id)?.inserts.map((e) => e.id) ?? []));
  const inserts: GraphNode[] = [];
  let head = MASTER_NODE;
  for (let i = 0; i < graph.nodes.length; i++) {
    const [only, extra] = normalIn(graph, head);
    const before = only && !extra ? findNode(graph, only.from) : undefined;
    if (before?.kind.type !== "effect" || taken.has(before.id) || normalOut(graph, before.id).length !== 1) break;
    inserts.unshift(before);
    head = before.id;
  }
  return { node, inserts, tail: MASTER_NODE, output: null, sends: [] };
}

/** Master input including its inserts: the first master insert or master itself. Output "to master" goes here, otherwise audio would bypass the limiter. */
export function masterInput(graph: Graph): Id {
  return masterModel(graph)?.inserts[0]?.id ?? MASTER_NODE;
}

export interface OutputTarget {
  id: Id;
  label: string;
}

/**
 * Where a strip's main output can go: master (via its inserts, the "Master" item) and buses that would
 * not close a cycle. The current output is always included, even if it is a shared effect (several tracks into one compressor),
 * otherwise the list could not show it.
 */
export function outputTargets(graph: Graph, strip: StripModel): OutputTarget[] {
  const master = masterInput(graph);
  const open = (id: Id) => id === strip.output?.to || (!isRoutedTo(strip, id) && checkEdge(graph, strip.tail, id, false) === null);
  const targets: OutputTarget[] = open(master) ? [{ id: master, label: "Master" }] : [];
  for (const n of graph.nodes) {
    if (n.id === master || !(n.kind.type === "bus" || n.id === strip.output?.to) || !open(n.id)) continue;
    targets.push({ id: n.id, label: nodeLabel(n) });
  }
  return targets;
}

/** Buses a send can be added to: not yet connected to the strip and not forming a cycle. */
export function sendTargets(graph: Graph, strip: StripModel): GraphNode[] {
  return graph.nodes.filter((n) => n.kind.type === "bus" && !isRoutedTo(strip, n.id) && checkEdge(graph, strip.tail, n.id, false) === null);
}

/** Whether the strip's audio already goes to node `to` via the main output or a send. */
function isRoutedTo(strip: StripModel, to: Id): boolean {
  return strip.output?.to === to || strip.sends.some((e) => e.to === to);
}

/** Only an edge from a source or bus can be pre-fader: a send after inserts (from an effect) is always post-fader. */
export function canBePreFader(graph: Graph, edge: GraphEdge): boolean {
  const from = findNode(graph, edge.from);
  return from !== undefined && hasStrip(from.kind);
}
