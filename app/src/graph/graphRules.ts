import type { Graph, GraphEdge, GraphNode, Id, NodeKind, Strip, Track } from "../api/types";
import { t } from "../i18n";

/** Engine table limits (as `MAX_NODES`/`MAX_EDGES`/`MAX_LEVEL` in Rust). */
export const MAX_NODES = 256;
export const MAX_EDGES = 1024;
export const MAX_LEVEL = 1.5;

/** Source node of a track (instrument or audio). */
export type SourceKind = Extract<NodeKind, { type: "instrument" | "audio" }>;

export const isSource = (kind: NodeKind): kind is SourceKind => kind.type === "instrument" || kind.type === "audio";

/** Whether the node receives sound: all but sources (as `NodeKind::accepts_input`). */
export const acceptsInput = (kind: NodeKind): boolean => !isSource(kind);

/** Whether the node passes sound on: all but the master (as `NodeKind::has_output`). */
export const hasOutput = (kind: NodeKind): boolean => kind.type !== "master";

/** Sources, buses and the master have volume (as `NodeKind::has_volume`). */
export const hasVolume = (kind: NodeKind): kind is Exclude<NodeKind, { type: "effect" }> => kind.type !== "effect";

/** Sources and buses have pan, mute and solo; the master only volume (as `NodeKind::has_strip`). */
export const hasStrip = (kind: NodeKind): kind is Extract<NodeKind, Strip> => isSource(kind) || kind.type === "bus";

/** Whether a connection from the node can be a pre-fader send: only nodes with a fader (as `Graph::check_pre_fader`). */
export const canPreFader = (kind: NodeKind): boolean => hasStrip(kind);

/** A source muted with M (its track's clips are drawn paler). */
export const isMutedSource = (kind: NodeKind): boolean => isSource(kind) && kind.mute;

export function findNode(graph: Graph, id: Id): GraphNode | undefined {
  return graph.nodes.find((n) => n.id === id);
}

export function outgoing(graph: Graph, id: Id): GraphEdge[] {
  return graph.edges.filter((e) => e.from === id);
}

export function incoming(graph: Graph, id: Id): GraphEdge[] {
  return graph.edges.filter((e) => e.to === id);
}

export function sourceOfTrack(graph: Graph, track: Id): GraphNode | undefined {
  return graph.nodes.find((n) => isSource(n.kind) && n.kind.track === track);
}

export function mutedTracks(tracks: Track[], graph: Graph): Set<Id> {
  return new Set(tracks.filter((t) => {
    const source = sourceOfTrack(graph, t.id);
    return source !== undefined && isMutedSource(source.kind);
  }).map((t) => t.id));
}

/** Whether `target` is reachable from `start` over connections of any kind. */
export function reaches(graph: Graph, start: Id, target: Id): boolean {
  const seen = new Set<Id>();
  const stack = [start];
  while (stack.length) {
    const current = stack.pop()!;
    if (current === target) return true;
    if (seen.has(current)) continue;
    seen.add(current);
    for (const edge of outgoing(graph, current)) stack.push(edge.to);
  }
  return false;
}

/** Whether `from → to` can be connected: `null` — yes, otherwise the reason (same wording as the backend). */
export function checkEdge(graph: Graph, from: Id, to: Id, sidechain: boolean): string | null {
  const source = findNode(graph, from);
  const problems = t().graph.problems;
  if (!source) return problems.nodeNotFound(from);
  const target = findNode(graph, to);
  if (!target) return problems.nodeNotFound(to);
  if (from === to) return problems.selfLoop;
  if (!hasOutput(source.kind)) return problems.masterNoOutput;
  if (!acceptsInput(target.kind)) return problems.sourceNoInput;
  if (sidechain && target.kind.type !== "effect") return problems.sidechainEffectOnly;
  if (reaches(graph, to, from)) return problems.cycle;
  return null;
}

/** All nodes whose sound reaches `id` (excluding `id`), by ascending id. */
export function upstream(graph: Graph, id: Id): Id[] {
  const found = new Set<Id>();
  const stack = [id];
  while (stack.length) {
    for (const edge of incoming(graph, stack.pop()!)) {
      if (!found.has(edge.from)) {
        found.add(edge.from);
        stack.push(edge.from);
      }
    }
  }
  return [...found].sort((a, b) => a - b);
}

/** Node label: its name, or its kind if the name is empty. */
export function nodeLabel(node: GraphNode): string {
  if (node.name) return node.name;
  switch (node.kind.type) {
    case "instrument":
    case "audio":
      return t().tracks.trackName(node.kind.track);
    case "effect":
      return node.kind.effect;
    case "bus":
      return t().graph.busName(node.id);
    case "master":
      return "Master";
  }
}

/** New bus name: "Bus N", N being the next number among existing buses. */
export function nextBusName(graph: Graph): string {
  return t().graph.busName(graph.nodes.filter((n) => n.kind.type === "bus").length + 1);
}

/** The sidechain key source of effect `effect`: where its first sidechain input comes from, or `null`. */
export function sidechainSource(graph: Graph, effect: Id): Id | null {
  return incoming(graph, effect).find((e) => e.sidechain)?.from ?? null;
}

/** Tracks and buses that can key the sidechain of `effect` (the connection breaks no graph rule, e.g. makes no loop). */
export function sidechainCandidates(graph: Graph, effect: Id): GraphNode[] {
  return graph.nodes.filter((n) => hasStrip(n.kind) && checkEdge(graph, n.id, effect, true) === null);
}
