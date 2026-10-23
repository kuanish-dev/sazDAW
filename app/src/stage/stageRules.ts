import type { Id, Project, Room } from "../api/types";
import { hasStrip } from "../graph/graphRules";
import { clamp } from "../common/math";
import { stripOrder } from "../mixer/mixerStrips";

/** A point of the stage, each 0 at the listener's head: `pan` −1…1 (wall to wall), `depth` −1…1 (the wall behind the
 *  listener to the wall ahead), `height` −1…1 (floor to ceiling). */
export interface Spot {
  pan: number;
  depth: number;
  height: number;
}

/** A point relative to the listener's head, m: `x` to the right, `y` forward, `z` up. */
export interface Position {
  x: number;
  y: number;
  z: number;
}

export interface RoomShape {
  width: number;
  /** From the wall behind the listener to the wall ahead (the listener stands halfway). */
  depth: number;
  height: number;
}

/** The direct sound keeps its level up to this distance, then falls 6 dB per doubling, m. */
const REFERENCE_DISTANCE = 2;
/** The closest a source counts as to the listener's head, m (nearer, it keeps this distance's level). */
const NEAREST = 0.5;
/** The closest a source comes to a wall, the floor or the ceiling, m. */
export const WALL_GAP = 0.5;

const grow = (small: number, large: number, size: number) => small * (large / small) ** clamp(size, 0, 1);

export const roomShape = (size: number): RoomShape => ({ width: grow(6, 30, size), depth: grow(5, 24, size), height: grow(3, 10, size) });

/** The usable stage inside the room: how far from the listener a source can stand along each axis. */
function bounds(shape: RoomShape) {
  return { halfWidth: shape.width / 2 - WALL_GAP, halfDepth: shape.depth / 2 - WALL_GAP, halfHeight: shape.height / 2 - WALL_GAP };
}

export function position(shape: RoomShape, spot: Spot): Position {
  const b = bounds(shape);
  return { x: clamp(spot.pan, -1, 1) * b.halfWidth, y: clamp(spot.depth, -1, 1) * b.halfDepth, z: clamp(spot.height, -1, 1) * b.halfHeight };
}

/** The stage pan and depth of a point of the room seen from above, `y` metres ahead of the listener (clamped to the stage). */
export function placeAt(shape: RoomShape, x: number, y: number): { pan: number; depth: number } {
  const b = bounds(shape);
  return { pan: clamp(x / b.halfWidth, -1, 1), depth: clamp(y / b.halfDepth, -1, 1) };
}

/** The stage height of a point `z` metres above ear height (clamped to the room). */
export const heightAt = (shape: RoomShape, z: number): number => clamp(z / bounds(shape).halfHeight, -1, 1);

/** The direct sound's level at `distance` m (at most 1). */
export const directGain = (distance: number): number => Math.min(1, REFERENCE_DISTANCE / distance);

/** What the listener hears from a spot: how far it is, how loud its direct sound is and from which direction. */
export interface Hearing {
  distance: number;
  levelDb: number;
  /** Degrees to the right (negative: to the left), −180…180: beyond ±90 the sound is behind. */
  azimuth: number;
  /** Degrees above ear height (negative: below). */
  elevation: number;
}

export function hearing(shape: RoomShape, spot: Spot): Hearing {
  const { x, y, z } = position(shape, spot);
  const distance = Math.max(NEAREST, Math.hypot(x, y, z));
  const degrees = (radians: number) => (radians * 180) / Math.PI;
  return {
    distance,
    levelDb: 20 * Math.log10(directGain(distance)),
    azimuth: degrees(Math.atan2(x, y)),
    elevation: degrees(Math.atan2(z, Math.hypot(x, y))),
  };
}

/** A channel the stage view shows: a track's source or a bus, and where it stands (`spot` absent: off the stage). */
export interface StageChannel {
  node: Id;
  name: string;
  /** The track it belongs to (`null` for a bus). */
  track: Id | null;
  pan: number;
  spot?: Spot;
}

/** Every channel that can stand on the stage, in mixer order: tracks, then buses. */
export function stageChannels(project: Project): StageChannel[] {
  return stripOrder(project).flatMap((id) => {
    const node = project.graph.nodes.find((n) => n.id === id);
    if (!node || !hasStrip(node.kind)) return [];
    const { kind } = node;
    const track = "track" in kind ? kind.track : null;
    const name = (track !== null ? project.tracks.find((t) => t.id === track)?.name : undefined) ?? node.name;
    return [{ node: id, name, track, pan: kind.pan, spot: kind.stage ? { pan: kind.pan, ...kind.stage } : undefined }];
  });
}

/** Places for `count` channels coming on the stage at once: an arc in front of the listener, like an ensemble. */
export function ensembleSpots(count: number): Spot[] {
  return Array.from({ length: count }, (_, i) => {
    const across = count === 1 ? 0 : (i / (count - 1)) * 2 - 1;
    return { pan: across * 0.8, depth: 0.5 + 0.4 * (1 - across * across), height: 0 };
  });
}

/** The room's measures as text: "16 × 13 × 6 m". */
export const roomMeasures = (room: Room): string => {
  const { width, depth, height } = roomShape(room.size);
  return `${Math.round(width)} × ${Math.round(depth)} × ${Math.round(height)}`;
};
