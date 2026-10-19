// Automation: lanes, their clips (patterns of points repeated over the clip), shapes, and a simplified live write
// (a touch while playing puts a point into the clip under the play position, or into a new clip from the bar start;
// one undo step per play pass).
import type { AutoPoint, AutomationClip, AutomationLane, Id, ParamTarget, Project, ShapeRequest } from "../../api/types";
import { autoClipAt, overlapsAutoClips, paramValue, sameTarget } from "../../automation/automationRules";
import {
  MAX_AUTOMATION_CLIPS, MAX_CLIP_LOOP_TICKS, MAX_POINTS, MIN_SHAPE_PERIOD, replaceRange, shapePointCount, shapePoints,
} from "../../automation/automationCurve";
import { hasStrip, hasVolume } from "../../graph/graphRules";
import { barStart } from "../../common/tempoMap";
import { t } from "../../i18n";
import type { BackendError } from "../../i18n/errors";
import { MAX_TICK } from "../clipRules";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** `daw_contracts::MAX_LANES`. */
const MAX_LANES = 256;

/** The backend's target checks: volume on sources, buses and master, pan on sources and buses, stage depth and height on
 * a source or bus standing on the stage, a parameter on an effect, a level on an existing connection. Returns the refusal, or `null` for a valid target. */
export function targetProblem(p: Project, target: ParamTarget): BackendError | null {
  const missing = (id: Id) => (p.graph.nodes.some((n) => n.id === id) ? null : fail("GR-001", { id }));
  if (target.type === "edgeLevel") {
    const { from, to, sidechain } = target;
    const connected = p.graph.edges.some((e) => e.from === from && e.to === to && e.sidechain === sidechain);
    return missing(from) ?? missing(to) ?? (connected ? null : fail("GR-120", { from, to }));
  }
  if (target.type === "instrumentParam") {
    const track = p.tracks.find((x) => x.id === target.track);
    if (!track) return fail("TR-001", { id: target.track });
    if (!track.instrument) return fail("TR-160", { id: track.id });
    return target.param in track.instrument.params ? null : fail("TR-113", { kind: track.instrument.kind, param: target.param });
  }
  const node = p.graph.nodes.find((n) => n.id === target.node);
  if (!node) return fail("GR-001", { id: target.node });
  const { kind } = node;
  if (target.type === "nodeVolume") return hasVolume(kind) ? null : fail("GR-115", { id: target.node });
  if (target.type === "nodePan") return hasStrip(kind) ? null : fail("GR-116", { id: target.node });
  if (target.type === "nodeWidth") return hasStrip(kind) ? null : fail("GR-126", { id: target.node });
  if (target.type === "stageDepth" || target.type === "stageHeight") {
    if (!hasStrip(kind)) return fail("GR-123", { id: target.node });
    return kind.stage ? null : fail("GR-124", { id: target.node });
  }
  if (kind.type !== "effect") return fail("GR-160", { id: target.node });
  return target.param in kind.params ? null : fail("GR-121", { kind: kind.effect, param: target.param });
}

/** `targetProblem` as a refusal: throws it. */
export function validateTarget(p: Project, target: ParamTarget) {
  const problem = targetProblem(p, target);
  if (problem) throw problem;
}

function laneOf(p: Project, id: Id): AutomationLane {
  const lane = p.automation.find((l) => l.id === id);
  if (!lane) throw fail("AT-010", { id });
  return lane;
}

function clipOf(lane: AutomationLane, id: Id): AutomationClip {
  const clip = lane.clips.find((c) => c.id === id);
  if (!clip) throw fail("AT-011", { id, lane: lane.id });
  return clip;
}

/** The backend's clip span checks: length and loop at least a tick, loop at most the maximum, inside the timeline, no
 * overlap with the lane's other clips. */
function checkSpan(lane: AutomationLane, start: number, length: number, loop: number, self?: Id) {
  if (length < 1 || loop < 1) throw fail("AT-116");
  if (loop > MAX_CLIP_LOOP_TICKS) throw fail("AT-118", { max: MAX_CLIP_LOOP_TICKS });
  if (start < 0 || start + length > MAX_TICK) throw fail("AT-117", { start, length });
  if (overlapsAutoClips(lane, start, length, self)) throw fail("AT-115", { start, end: start + length });
}

function addClip(lane: AutomationLane, clip: AutomationClip) {
  if (lane.clips.length >= MAX_AUTOMATION_CLIPS) throw fail("AT-212", { max: MAX_AUTOMATION_CLIPS });
  lane.clips = [...lane.clips, clip].sort((a, b) => a.startTick - b.startTick);
}

/** Points as the backend stores them: sorted, one per tick (the last wins), curve clamped, inside the pattern. */
function normalized(points: AutoPoint[], loop: number): AutoPoint[] {
  if (points.length > MAX_POINTS) throw fail("AT-211", { max: MAX_POINTS });
  const byTick = new Map<number, AutoPoint>();
  for (const point of points) {
    if (!Number.isFinite(point.value)) throw fail("AT-110", { value: point.value });
    if (point.curve !== undefined && !Number.isFinite(point.curve)) throw fail("AT-114", { curve: point.curve });
    if (point.tick > loop) throw fail("AT-119", { tick: point.tick, loop });
    byTick.set(point.tick, { tick: point.tick, value: point.value, curve: Math.max(-1, Math.min(1, point.curve ?? 0)) });
  }
  return [...byTick.values()].sort((a, b) => a.tick - b.tick);
}

export function automationHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const history = () => t().mockEdits.history;
  /** The clip and project revision of the last `setPoints`: the next one of the same clip right after it joins its undo step. */
  let lastPoints: { lane: Id; clip: Id; revision: number } | null = null;

  h["automation.addLane"] = ({ target }) => w.edit(history().addAutomation, (p) => {
    validateTarget(p, target);
    if (p.automation.some((l) => sameTarget(l.target, target))) throw fail("AT-111");
    if (p.automation.length >= MAX_LANES) throw fail("AT-210", { max: MAX_LANES });
    const id = w.id(p);
    p.automation.push({ id, target, clips: [] });
    return id;
  });
  h["automation.removeLane"] = ({ id }) => w.edit(history().removeAutomation, (p) => { p.automation = p.automation.filter((l) => l.id !== id); });

  h["automation.addClip"] = ({ lane: laneId, startTick, lengthTicks }) => w.edit(history().addAutomationClip, (p) => {
    const lane = laneOf(p, laneId);
    const loop = Math.min(lengthTicks, MAX_CLIP_LOOP_TICKS);
    checkSpan(lane, startTick, lengthTicks, loop);
    const id = w.id(p);
    addClip(lane, { id, startTick, lengthTicks, loopTicks: loop, points: [{ tick: 0, value: paramValue(p, lane.target), curve: 0 }] });
    return id;
  });
  h["automation.editClip"] = ({ lane: laneId, clip: clipId, startTick, lengthTicks, loopTicks }) => w.edit(history().editAutomationClip, (p) => {
    const lane = laneOf(p, laneId);
    const clip = clipOf(lane, clipId);
    const next = {
      startTick: (startTick as number | undefined) ?? clip.startTick,
      lengthTicks: (lengthTicks as number | undefined) ?? clip.lengthTicks,
      loopTicks: (loopTicks as number | undefined) ?? clip.loopTicks,
    };
    checkSpan(lane, next.startTick, next.lengthTicks, next.loopTicks, clip.id);
    Object.assign(clip, next);
    clip.points = clip.points.filter((pt) => pt.tick <= next.loopTicks);
    lane.clips.sort((a, b) => a.startTick - b.startTick);
    return w.clone(clip);
  });
  h["automation.duplicateClip"] = ({ lane: laneId, clip: clipId }) => w.edit(history().duplicateAutomationClip, (p) => {
    const lane = laneOf(p, laneId);
    const clip = clipOf(lane, clipId);
    const start = clip.startTick + clip.lengthTicks;
    checkSpan(lane, start, clip.lengthTicks, clip.loopTicks);
    const id = w.id(p);
    addClip(lane, { ...w.clone(clip), id, startTick: start });
    return id;
  });
  h["automation.removeClip"] = ({ lane: laneId, clip: clipId }) => w.edit(history().removeAutomationClip, (p) => {
    const lane = laneOf(p, laneId);
    clipOf(lane, clipId);
    lane.clips = lane.clips.filter((c) => c.id !== clipId);
  });
  h["automation.setPoints"] = ({ lane: laneId, clip: clipId, points }) => {
    const apply = (p: Project) => {
      const clip = clipOf(laneOf(p, laneId), clipId);
      clip.points = normalized(points as AutoPoint[], clip.loopTicks);
    };
    const last = lastPoints;
    const joins = last !== null && last.lane === laneId && last.clip === clipId && last.revision === w.revision;
    if (joins) {
      apply(w.project);
      w.dirty = true;
      w.touch();
    } else {
      w.edit(history().automationPoints, apply);
    }
    lastPoints = { lane: laneId, clip: clipId, revision: w.revision };
  };
  h["automation.drawShape"] = ({ lane: laneId, clip: clipId, ...request }) => {
    const r = request as ShapeRequest;
    if (r.fromTick >= r.toTick) throw fail("AT-112", { from: r.fromTick, to: r.toTick });
    const ramp = r.shape === "rampUp" || r.shape === "rampDown";
    if (!ramp && r.periodTicks < MIN_SHAPE_PERIOD) throw fail("AT-113", { min: MIN_SHAPE_PERIOD });
    if (shapePointCount(r) > MAX_POINTS) throw fail("AT-211", { max: MAX_POINTS });
    return w.edit(history().drawAutomationShape, (p) => {
      const clip = clipOf(laneOf(p, laneId), clipId);
      if (r.toTick > clip.loopTicks) throw fail("AT-119", { tick: r.toTick, loop: clip.loopTicks });
      const next = replaceRange(clip.points, r.fromTick, r.toTick, shapePoints(r));
      if (next.length > MAX_POINTS) throw fail("AT-211", { max: MAX_POINTS });
      clip.points = next;
    });
  };
  /** Clips this play pass created outside existing clips: they grow as writing goes on (loop = length). */
  const created = new Set<Id>();
  /** Outside clips (nothing lies between the previous clip and `tick`): the clip this pass created before `tick`, grown
   * to it, or a new one from the bar start. */
  const writtenClip = (lane: AutomationLane, tick: number, p: Project): AutomationClip => {
    const before = lane.clips.filter((c) => c.startTick + c.lengthTicks <= tick);
    const previous = before[before.length - 1];
    if (previous && created.has(previous.id) && tick + 1 - previous.startTick <= MAX_CLIP_LOOP_TICKS) {
      previous.lengthTicks = tick + 1 - previous.startTick;
      previous.loopTicks = previous.lengthTicks;
      return previous;
    }
    const start = Math.max(barStart(p.tempoMap, tick), previous ? previous.startTick + previous.lengthTicks : 0);
    const clip: AutomationClip = { id: w.id(p), startTick: start, lengthTicks: tick + 1 - start, loopTicks: tick + 1 - start, points: [] };
    addClip(lane, clip);
    created.add(clip.id);
    return clip;
  };
  h["automation.writeMode"] = () => w.automationWrite;
  h["automation.setWriteMode"] = ({ mode }) => {
    w.automationWrite = mode;
    w.automationPass = false;
  };
  // The real writer follows the play position and holds the lane; the mock only drops a point per touch: into the clip
  // under the position (pattern-local), or into a clip from the bar start that grows to the position.
  h["automation.touch"] = ({ target, value }) => {
    if (w.automationWrite === "off" || w.state !== "playing") {
      w.automationPass = false;
      return;
    }
    const write = (p: Project) => {
      let lane = p.automation.find((l) => sameTarget(l.target, target));
      if (!lane) {
        validateTarget(p, target);
        lane = { id: w.id(p), target, clips: [] };
        p.automation.push(lane);
      }
      const tick = Math.floor(w.position);
      const clip = autoClipAt(lane, tick) ?? writtenClip(lane, tick, p);
      const local = (tick - clip.startTick) % clip.loopTicks;
      clip.points = [...clip.points.filter((pt) => pt.tick !== local), { tick: local, value }].sort((a, b) => a.tick - b.tick);
    };
    if (!w.automationPass) {
      w.automationPass = true;
      created.clear();
      w.edit(history().writeAutomation, write);
      return;
    }
    write(w.project);
    w.touch();
  };
  h["automation.release"] = () => undefined;
  h["automation.releaseAll"] = () => undefined;
  return h;
}
