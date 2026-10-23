import {
  MAX_WIDTH, PPQ, type AutoPoint, type AutomationClip, type AutomationLane, type Catalog, type Id, type ParamTarget, type Project,
} from "../api/types";
import { t } from "../i18n";
import { effectInfo, instrumentInfo, paramInfo } from "../common/catalog";
import { findNode, hasStrip, hasVolume, MAX_LEVEL, nodeLabel, sourceOfTrack } from "../graph/graphRules";
import { stripModel } from "../mixer/mixerStrips";
import { clamp } from "../common/math";
import { BAR_GRID } from "../common/time";
import type { OwnedLane } from "../tracks/trackRows";
import { trackById } from "../tracks/trackRules";

export interface Range {
  min: number;
  max: number;
  /** Whole values only (choice index, switch): curve points land on them. */
  whole?: boolean;
}

const UNIT_RANGE: Readonly<Range> = { min: 0, max: 1 } as const;

export function sameTarget(a: ParamTarget, b: ParamTarget): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * The target's parameter exists now (the backend's `validate_target`): volume on sources, buses and master, pan on
 * sources and buses, stage depth and height on a source or bus standing on the stage, a parameter of an effect or a
 * track instrument, the level of an existing connection.
 */
export function targetExists(project: Project, target: ParamTarget): boolean {
  if (target.type === "edgeLevel") {
    const { from, to, sidechain } = target;
    return project.graph.edges.some((e) => e.from === from && e.to === to && e.sidechain === sidechain);
  }
  if (target.type === "instrumentParam") {
    const params = trackById(project, target.track)?.instrument?.params;
    return params !== undefined && target.param in params;
  }
  const kind = findNode(project.graph, target.node)?.kind;
  if (!kind) return false;
  if (target.type === "nodeVolume") return hasVolume(kind);
  if (target.type === "nodePan" || target.type === "nodeWidth") return hasStrip(kind);
  if (target.type === "stageDepth" || target.type === "stageHeight") return hasStrip(kind) && kind.stage !== undefined;
  return kind.type === "effect" && target.param in kind.params;
}

/** Description of the kind owning the target's parameter (node effect or track instrument). */
function kindOf(target: ParamTarget, project: Project, catalog: Catalog | null) {
  if (target.type === "effectParam") {
    const kind = findNode(project.graph, target.node)?.kind;
    return effectInfo(catalog, kind?.type === "effect" ? kind.effect : undefined);
  }
  if (target.type === "instrumentParam") return instrumentInfo(catalog, trackById(project, target.track)?.instrument?.kind);
  return undefined;
}

/** Allowed parameter range for the curve axis. */
export function targetRange(target: ParamTarget, project: Project, catalog: Catalog | null): Range {
  switch (target.type) {
    case "nodeVolume":
    case "edgeLevel":
      return { min: 0, max: MAX_LEVEL };
    case "nodeWidth":
      return { min: 0, max: MAX_WIDTH };
    case "nodePan":
    case "stageDepth":
    case "stageHeight":
      return { min: -1, max: 1 };
    default: {
      const info = paramInfo(kindOf(target, project, catalog), target.param);
      return info ? { min: info.min, max: info.max, whole: info.kind === "choice" || info.kind === "switch" } : UNIT_RANGE;
    }
  }
}

function nodeName(project: Project, id: number): string {
  const node = findNode(project.graph, id);
  return node ? nodeLabel(node) : `#${id}`;
}

export function targetLabel(target: ParamTarget, project: Project, catalog: Catalog | null): string {
  switch (target.type) {
    case "nodeVolume":
      return t().automation.volumeOf(nodeName(project, target.node));
    case "nodePan":
      return t().automation.panOf(nodeName(project, target.node));
    case "nodeWidth":
      return t().automation.widthOf(nodeName(project, target.node));
    case "stageDepth":
      return t().automation.stageDepthOf(nodeName(project, target.node));
    case "stageHeight":
      return t().automation.stageHeightOf(nodeName(project, target.node));
    case "edgeLevel":
      return t().automation.edgeLevel(target.sidechain, nodeName(project, target.from), nodeName(project, target.to));
    case "effectParam":
    case "instrumentParam": {
      const label = paramInfo(kindOf(target, project, catalog), target.param)?.label ?? target.param;
      const owner = target.type === "effectParam"
        ? nodeName(project, target.node)
        : (trackById(project, target.track)?.name ?? `#${target.track}`);
      return `${owner} · ${label}`;
    }
  }
}

/** Point time snap: with the "Bar" grid points snap to beats, otherwise to the grid. */
export const pointSnap = (grid: number): number => (grid === BAR_GRID ? PPQ : grid);

/** Value → curve height fraction 0…1 (0 — bottom). */
export const valueToRatio = (value: number, range: Range): number => (value - range.min) / (range.max - range.min);

/** Curve height fraction → value in range (whole ranges round to the nearest integer). */
export function ratioToValue(ratio: number, range: Range): number {
  const value = range.min + clamp(ratio, 0, 1) * (range.max - range.min);
  return range.whole ? Math.round(value) : value;
}

/** Inserts a point keeping time order; returns the new points and the inserted index. */
export function insertPoint(points: AutoPoint[], point: AutoPoint): { points: AutoPoint[]; index: number } {
  const after = points.findIndex((p) => p.tick > point.tick);
  const index = after < 0 ? points.length : after;
  return { points: [...points.slice(0, index), point, ...points.slice(index)], index };
}

/** A selected automation lane row and, optionally, one of its clips (its pattern opens in the clip panel). */
export interface AutomationSelection {
  lane: Id;
  clip: Id | null;
}

export const laneById = (project: Project, id: Id | null): AutomationLane | undefined => project.automation.find((l) => l.id === id);

export const laneOfTarget = (project: Project, target: ParamTarget): AutomationLane | undefined =>
  project.automation.find((l) => sameTarget(l.target, target));

/** The selected clip with its lane; `undefined` if no clip is selected or it is gone. */
export function automationClipOf(project: Project, ref: AutomationSelection | null): { lane: AutomationLane; clip: AutomationClip } | undefined {
  const lane = ref ? laneById(project, ref.lane) : undefined;
  const clip = lane?.clips.find((c) => c.id === ref?.clip);
  return lane && clip ? { lane, clip } : undefined;
}

export const autoClipEnd = (clip: AutomationClip): number => clip.startTick + clip.lengthTicks;

/** The clip of the lane under `tick`. */
export const autoClipAt = (lane: AutomationLane, tick: number): AutomationClip | undefined =>
  lane.clips.find((c) => tick >= c.startTick && tick < autoClipEnd(c));

/** Whether `start…start+length` overlaps a clip of the lane other than `except` (clips of a lane never overlap). */
export const overlapsAutoClips = (lane: AutomationLane, start: number, length: number, except?: Id): boolean =>
  lane.clips.some((c) => c.id !== except && c.startTick < start + length && start < autoClipEnd(c));

/**
 * Room for a new clip at `tick`: from `tick` — or the end of a clip covering it — up to the next clip's start
 * (`Infinity` — no clip after it). Empty when `to <= from`.
 */
export function freeSpanAt(lane: AutomationLane, tick: number): { from: number; to: number } {
  const previousEnd = Math.max(0, ...lane.clips.filter((c) => c.startTick <= tick).map(autoClipEnd));
  const from = Math.max(tick, previousEnd);
  return { from, to: lane.clips.find((c) => c.startTick > tick)?.startTick ?? Number.POSITIVE_INFINITY };
}

/** The parameter's own value in the project (what a new automation clip starts from); `0` if the target is gone. */
export function paramValue(project: Project, target: ParamTarget): number {
  switch (target.type) {
    case "edgeLevel":
      return project.graph.edges.find((e) => e.from === target.from && e.to === target.to && e.sidechain === target.sidechain)?.level ?? 0;
    case "instrumentParam":
      return trackById(project, target.track)?.instrument?.params[target.param] ?? 0;
    default: {
      const kind = findNode(project.graph, target.node)?.kind;
      if (!kind) return 0;
      if (target.type === "effectParam") return kind.type === "effect" ? (kind.params[target.param] ?? 0) : 0;
      if (target.type === "nodePan") return "pan" in kind ? kind.pan : 0;
      if (target.type === "nodeWidth") return hasStrip(kind) ? kind.width : 1;
      if (target.type === "stageDepth") return hasStrip(kind) ? (kind.stage?.depth ?? 0) : 0;
      if (target.type === "stageHeight") return hasStrip(kind) ? (kind.stage?.height ?? 0) : 0;
      return "volume" in kind ? kind.volume : 0;
    }
  }
}

/** The track whose strip holds node `node`: its source or one of its inserts; `null` — a bus, master or shared effect. */
function trackOfNode(project: Project, node: Id): Id | null {
  for (const track of project.tracks) {
    const source = sourceOfTrack(project.graph, track.id);
    if (!source) continue;
    if (source.id === node || stripModel(project.graph, source.id)?.inserts.some((n) => n.id === node)) return track.id;
  }
  return null;
}

/** The track a lane belongs to (its row stands under that track): an instrument parameter's track, the track whose
 * strip holds the node (a connection — its start); `null` — buses and master. */
export function laneTrack(project: Project, target: ParamTarget): Id | null {
  switch (target.type) {
    case "instrumentParam":
      return target.track;
    case "edgeLevel":
      return trackOfNode(project, target.from);
    default:
      return trackOfNode(project, target.node);
  }
}

/** The project's lanes, in project order, with their tracks — input of `trackRows`. */
export const ownedLanes = (project: Project): OwnedLane[] => project.automation.map((lane) => ({ lane, track: laneTrack(project, lane.target) }));
