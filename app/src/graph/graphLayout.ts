// Node editor geometry: card sizes, port positions, edge curves, column layout, placement of an inserted effect,
// port hit-testing and screen -> world coordinate conversion. Pure functions without DOM.
import type { GraphEdge, GraphNode, Id } from "../api/types";
import { acceptsInput, hasOutput, isSource } from "./graphRules";
import { clamp } from "../common/math";

export interface Point {
  x: number;
  y: number;
}

/** Camera position and zoom: screen = world * zoom + (x, y). */
export interface View {
  x: number;
  y: number;
  zoom: number;
}

export const CARD_WIDTH = 176;
export const HEADER_HEIGHT = 28;
/** Distance (in world coordinates) at which the pointer snaps to a port. */
export const PORT_HIT_RADIUS = 14;
export const MIN_ZOOM = 0.3;
export const MAX_ZOOM = 2;

const COLUMN_GAP = 90;
const ROW_GAP = 24;
const MARGIN = 40;
/** Card body height below the header: a source has a label, fader and M/S; a bus and the master have a fader; an effect has a bypass row. */
const SOURCE_BODY = 120;
const MIX_BODY = 96;
const EFFECT_BODY = 44;
/** The sidechain input sits at the card's bottom edge, this far from the bottom. */
const SIDECHAIN_FROM_BOTTOM = 12;
/** Edge curvature: the curve's horizontal handles are half the distance, but no less than this. */
const MIN_CURVE_HANDLE = 40;

/** Card height depends on what the card shows (fader and meter on sources, buses and the master). */
export function cardHeight(node: GraphNode): number {
  switch (node.kind.type) {
    case "instrument":
    case "audio":
      return HEADER_HEIGHT + SOURCE_BODY;
    case "bus":
    case "master":
      return HEADER_HEIGHT + MIX_BODY;
    case "effect":
      return HEADER_HEIGHT + EFFECT_BODY;
  }
}

/** Only effects have a sidechain input. */
export const hasSidechain = (node: GraphNode): boolean => node.kind.type === "effect";

/** Sidechain input height from the card top. */
export const sidechainTop = (node: GraphNode): number => cardHeight(node) - SIDECHAIN_FROM_BOTTOM;

/** The input is on the left in the header; the sidechain input is on the left near the bottom. */
export function inPort(node: GraphNode, sidechain = false): Point {
  return { x: node.x, y: sidechain ? node.y + sidechainTop(node) : node.y + HEADER_HEIGHT / 2 };
}

export function outPort(node: GraphNode): Point {
  return { x: node.x + CARD_WIDTH, y: node.y + HEADER_HEIGHT / 2 };
}

/** Bezier curve from output to input; horizontal handles keep the bend smooth both forwards and backwards. */
export function bezierPath(a: Point, b: Point): string {
  const [c1, c2] = controls(a, b);
  return `M ${a.x} ${a.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${b.x} ${b.y}`;
}

function controls(a: Point, b: Point): [Point, Point] {
  const dx = Math.max(MIN_CURVE_HANDLE, Math.abs(b.x - a.x) * 0.5);
  return [{ x: a.x + dx, y: a.y }, { x: b.x - dx, y: b.y }];
}

/** Point on the curve at t from 0 to 1 (the edge midpoint hosts the level slider). */
export function bezierPoint(a: Point, b: Point, t: number): Point {
  const [c1, c2] = controls(a, b);
  const u = 1 - t;
  const mix = (p0: number, p1: number, p2: number, p3: number) =>
    u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
  return { x: mix(a.x, c1.x, c2.x, b.x), y: mix(a.y, c1.y, c2.y, b.y) };
}

/** Input port under the pointer: `{ id, sidechain }` of the nearest one within the radius, or `null`. */
export function hitInputPort(nodes: GraphNode[], p: Point, radius = PORT_HIT_RADIUS): { id: Id; sidechain: boolean } | null {
  let best: { id: Id; sidechain: boolean } | null = null;
  let bestDistance = radius;
  const consider = (id: Id, sidechain: boolean, port: Point) => {
    const distance = Math.hypot(port.x - p.x, port.y - p.y);
    if (distance <= bestDistance) {
      bestDistance = distance;
      best = { id, sidechain };
    }
  };
  for (const node of nodes) {
    if (acceptsInput(node.kind)) consider(node.id, false, inPort(node));
    if (hasSidechain(node)) consider(node.id, true, inPort(node, true));
  }
  return best;
}

/** Sources go in track order (`tracks`); other nodes keep their order. */
function sortedByTrack(nodes: GraphNode[], tracks: Id[]): GraphNode[] {
  const index = (node: GraphNode) => (isSource(node.kind) ? tracks.indexOf(node.kind.track) : -1);
  const position = (node: GraphNode) => (index(node) < 0 ? Number.MAX_SAFE_INTEGER : index(node));
  return [...nodes].sort((a, b) => position(a) - position(b));
}

/**
 * Node column along the signal path: sources are 0, an effect or bus is one right of its rightmost input (at least 1),
 * the master is right of everything. The graph is acyclic (the backend checks this); `visiting` only guards against loops.
 */
function columns(nodes: GraphNode[], edges: GraphEdge[]): Map<Id, number> {
  const found = new Map<Id, number>();
  const visiting = new Set<Id>();
  const kinds = new Map(nodes.map((n) => [n.id, n.kind]));
  const columnOf = (id: Id): number => {
    const known = found.get(id);
    if (known !== undefined) return known;
    const kind = kinds.get(id);
    if (!kind || isSource(kind) || visiting.has(id)) return 0;
    visiting.add(id);
    const inputs = edges.filter((e) => e.to === id && kinds.has(e.from)).map((e) => columnOf(e.from) + 1);
    visiting.delete(id);
    const column = Math.max(1, ...inputs);
    found.set(id, column);
    return column;
  };
  for (const node of nodes) if (node.kind.type !== "master") columnOf(node.id);
  const last = Math.max(0, ...found.values()) + 1;
  for (const node of nodes) if (node.kind.type === "master") found.set(node.id, last);
  return found;
}

/** Column layout along the signal path (sources | processing by depth | master); sources in track order. */
export function autoLayout(nodes: GraphNode[], edges: GraphEdge[], tracks: Id[] = []): { id: Id; x: number; y: number }[] {
  const columnOf = columns(nodes, edges);
  const next: number[] = [];
  return sortedByTrack(nodes, tracks).map((node) => {
    const column = columnOf.get(node.id) ?? 0;
    const y = next[column] ?? MARGIN;
    next[column] = y + cardHeight(node) + ROW_GAP;
    return { id: node.id, x: MARGIN + column * (CARD_WIDTH + COLUMN_GAP), y };
  });
}

/** Gap between a node and an effect inserted next to it. */
const INSERT_GAP = 30;

/**
 * Where to place an effect inserted at a node: after the node, to its right; before the master (it has no output), below it,
 * in the same column, which is usually free (buses sit to the left).
 */
export function insertedEffectPosition(node: GraphNode): Point {
  if (hasOutput(node.kind)) return { x: Math.round(node.x + CARD_WIDTH + INSERT_GAP), y: Math.round(node.y) };
  return { x: Math.round(node.x), y: Math.round(node.y + cardHeight(node) + INSERT_GAP) };
}

export function toWorld(view: View, screen: Point): Point {
  return { x: (screen.x - view.x) / view.zoom, y: (screen.y - view.y) / view.zoom };
}

/** Zoom around a screen point: the world point under the pointer stays in place. */
export function zoomAt(view: View, screen: Point, factor: number): View {
  const zoom = clamp(view.zoom * factor, MIN_ZOOM, MAX_ZOOM);
  const world = toWorld(view, screen);
  return { zoom, x: screen.x - world.x * zoom, y: screen.y - world.y * zoom };
}
