// The audio graph: effects, buses, connections, volume. Connection rules come from domain/graph (as in the backend).
import { MAX_CHANNEL_DELAY_MS, MAX_WIDTH, MASTER_NODE, type GraphEdge, type Id, type KindInfo, type Params, type Project } from "../../api/types";
import { neutralUtility } from "../../domain/channelUtility";
import { acceptsInput, canPreFader, hasStrip, hasVolume, MAX_LEVEL } from "../../domain/graph";
import { clamp } from "../../domain/math";
import { t } from "../../i18n";
import { fitParam, paramsFor } from "../catalog";
import { type ErrorCode, fail } from "../errors";
import { checkEdge, dropNode, dropStaleEdgeLanes, limitEdges, limitNodes, moveEdgeLane, nodeOf } from "../graphOps";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const NEW_BUS_VOLUME = 0.8;
/** The catalog param that makes an effect read its sidechain key (as `SIDECHAIN_PARAM` in the backend's `graph.rs`). */
const SIDECHAIN_PARAM = "sidechain";

/** `value`, or the backend's "not a finite number" error `code`. */
function finite(code: ErrorCode, value: number): number {
  if (!Number.isFinite(value)) throw fail(code, { value });
  return value;
}

const checkPosition = (x: number, y: number) => {
  finite("GR-110", x);
  finite("GR-110", y);
};

/** A node with a mixer strip (source or bus), otherwise the backend error `code` ("node … has no pan" and the like). */
function strip(p: Project, id: Id, code: ErrorCode) {
  const kind = nodeOf(p, id).kind;
  if (!hasStrip(kind)) throw fail(code, { id });
  return kind;
}

export function graphHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const effectKind = (effect: string) => w.must(w.catalog.effects.find((k) => k.kind === effect), fail("GR-002", { kind: effect }));
  /** Catalog defaults with `overrides` on top, checked before the edit as in the backend's `new_effect`. */
  const effectParams = (info: KindInfo, overrides: Params = {}): Params => {
    for (const value of Object.values(overrides)) finite("GR-114", value);
    return paramsFor(info, overrides, "GR-121");
  };
  const pushEffect = (p: Project, info: KindInfo, params: Params, x: number, y: number): Id => {
    const id = w.id(p);
    p.graph.nodes.push({ id, name: info.label, kind: { type: "effect", effect: info.kind, bypass: false, params }, x, y });
    return id;
  };
  h["graph.addEffect"] = ({ effect, params, x, y }) => {
    const info = effectKind(effect);
    const values = effectParams(info, params);
    checkPosition(x, y);
    return w.edit(t().mock.undo.addEffect, (p) => {
      limitNodes(p);
      return pushEffect(p, info, values, x, y);
    });
  };
  h["graph.addBus"] = ({ name, x, y }) => {
    checkPosition(x, y);
    return w.edit(t().mock.undo.addBus, (p) => {
      limitNodes(p);
      limitEdges(p);
      const id = w.id(p);
      p.graph.nodes.push({ id, name, kind: { type: "bus", volume: NEW_BUS_VOLUME, pan: 0, mute: false, solo: false, ...neutralUtility() }, x, y });
      p.graph.edges.push({ from: id, to: MASTER_NODE, level: 1, sidechain: false, preFader: false });
      return id;
    });
  };
  h["graph.removeNode"] = ({ id }) => w.edit(t().mock.undo.removeNode, (p) => {
    const kind = nodeOf(p, id).kind.type;
    if (kind !== "effect" && kind !== "bus") throw fail("GR-119", { id });
    dropNode(p, id);
  });
  h["graph.renameNode"] = ({ id, name }) => w.edit(t().mock.undo.renameNode, (p) => { nodeOf(p, id).name = name; });
  h["graph.moveNode"] = ({ id, x, y }) => {
    checkPosition(x, y);
    return w.edit(t().mock.undo.moveNode, (p) => { const node = nodeOf(p, id); node.x = x; node.y = y; });
  };
  h["graph.connect"] = ({ from, to, level, sidechain }) => {
    const value = clamp(finite("GR-111", level), 0, MAX_LEVEL);
    return w.edit(t().mock.undo.connect, (p) => {
      checkEdge(p, from, to, sidechain);
      const same = p.graph.edges.find((e) => e.from === from && e.to === to && e.sidechain === sidechain);
      if (same) {
        same.level = value;
        return;
      }
      limitEdges(p);
      p.graph.edges.push({ from, to, level: value, sidechain, preFader: false });
    });
  };
  h["graph.disconnect"] = ({ from, to, sidechain }) => w.edit(t().mock.undo.disconnect, (p) => {
    const before = p.graph.edges.length;
    p.graph.edges = p.graph.edges.filter((e) => !(e.from === from && e.to === to && e.sidechain === sidechain));
    if (p.graph.edges.length === before) throw fail("GR-120", { from, to });
    dropStaleEdgeLanes(p);
  });
  h["graph.setEdgePreFader"] = ({ from, to, preFader }) => w.edit(t().mock.undo.sendPrePost, (p) => {
    nodeOf(p, to);
    if (!canPreFader(nodeOf(p, from).kind)) throw fail("GR-106");
    const edge = p.graph.edges.find((e) => e.from === from && e.to === to && !e.sidechain);
    if (!edge) throw fail("GR-120", { from, to });
    edge.preFader = preFader;
  });
  h["graph.setSidechain"] = ({ effect, source }) => {
    const current = nodeOf(w.project, effect).kind;
    if (current.type !== "effect") throw fail("GR-160", { id: effect });
    const kind = current.effect;
    const info = w.catalog.effects.find((k) => k.kind === kind)?.params.find((x) => x.name === SIDECHAIN_PARAM);
    if (!info) throw fail("GR-122", { kind });
    return w.edit(t().mock.undo.sidechainSource, (p) => {
      p.graph.edges = p.graph.edges.filter((e) => !(e.to === effect && e.sidechain));
      if (source !== null) {
        checkEdge(p, source, effect, true);
        limitEdges(p);
        p.graph.edges.push({ from: source, to: effect, level: 1, sidechain: true, preFader: false });
      }
      dropStaleEdgeLanes(p);
      const node = nodeOf(p, effect).kind;
      if (node.type === "effect") node.params[SIDECHAIN_PARAM] = fitParam(info, source !== null ? 1 : 0);
    });
  };
  h["graph.setVolume"] = ({ id, volume }) => {
    const value = clamp(finite("GR-112", volume), 0, MAX_LEVEL);
    return w.edit(t().mock.undo.volume, (p) => {
      const kind = nodeOf(p, id).kind;
      if (!hasVolume(kind)) throw fail("GR-115", { id });
      kind.volume = value;
    });
  };
  h["graph.setPan"] = ({ id, pan }) => {
    const value = clamp(finite("GR-113", pan), -1, 1);
    return w.edit(t().mock.undo.pan, (p) => { strip(p, id, "GR-116").pan = value; });
  };
  h["graph.setMute"] = ({ id, mute }) => w.edit(t().mock.undo.mute, (p) => { strip(p, id, "GR-117").mute = mute; });
  h["graph.setSolo"] = ({ id, solo }) => w.edit(t().mock.undo.solo, (p) => { strip(p, id, "GR-118").solo = solo; });
  h["graph.setPolarity"] = ({ id, invertLeft, invertRight }) =>
    w.edit(t().mock.undo.polarity, (p) => { strip(p, id, "GR-126").polarity = { invertLeft, invertRight }; });
  h["graph.setWidth"] = ({ id, width }) => {
    const value = clamp(finite("GR-127", width), 0, MAX_WIDTH);
    return w.edit(t().mock.undo.width, (p) => { strip(p, id, "GR-126").width = value; }, `width:${id}`);
  };
  h["graph.setMono"] = ({ id, mono }) => w.edit(t().mock.undo.mono, (p) => { strip(p, id, "GR-126").mono = mono; });
  h["graph.setDelay"] = ({ id, ms }) => {
    const value = clamp(finite("GR-128", ms), -MAX_CHANNEL_DELAY_MS, MAX_CHANNEL_DELAY_MS);
    return w.edit(t().mock.undo.channelDelay, (p) => { strip(p, id, "GR-126").delayMs = value; }, `delay:${id}`);
  };
  h["graph.setEffectParam"] = ({ id, name, value }) => {
    finite("GR-114", value);
    const current = nodeOf(w.project, id).kind;
    if (current.type !== "effect") throw fail("GR-160", { id });
    const effect = current.effect;
    const info = w.catalog.effects.find((k) => k.kind === effect)?.params.find((x) => x.name === name);
    if (!info) throw fail("GR-121", { kind: effect, param: name });
    return w.edit(t().mock.undo.effectParam, (p) => {
      const kind = nodeOf(p, id).kind;
      if (kind.type === "effect") kind.params[name] = fitParam(info, value);
    });
  };
  h["graph.setEffectBypass"] = ({ id, bypass }) => w.edit(t().mock.undo.bypass, (p) => {
    const kind = nodeOf(p, id).kind;
    if (kind.type !== "effect") throw fail("GR-160", { id });
    kind.bypass = bypass;
  });
  h["graph.insertEffect"] = ({ after, effect, params, x, y }) => {
    const info = effectKind(effect);
    const values = effectParams(info, params);
    checkPosition(x, y);
    return w.edit(t().mock.undo.insertEffect, (p) => {
      nodeOf(p, after);
      limitNodes(p);
      limitEdges(p);
      // Post-fader outputs move to the effect with the same levels; sidechain and pre-fader sends stay on the node.
      const path = (e: GraphEdge) => e.from === after && !e.sidechain && !e.preFader;
      const moved = p.graph.edges.filter(path);
      p.graph.edges = p.graph.edges.filter((e) => !path(e));
      const id = pushEffect(p, info, values, x, y);
      checkEdge(p, after, id, false);
      p.graph.edges.push({ from: after, to: id, level: 1, sidechain: false, preFader: false });
      for (const edge of moved) {
        checkEdge(p, id, edge.to, false);
        p.graph.edges.push({ ...edge, from: id });
        moveEdgeLane(p, edge, id, edge.to);
      }
      return id;
    });
  };
  h["graph.insertEffectBefore"] = ({ node, effect, params, x, y }) => {
    const info = effectKind(effect);
    const values = effectParams(info, params);
    checkPosition(x, y);
    return w.edit(t().mock.undo.insertEffect, (p) => {
      if (!acceptsInput(nodeOf(p, node).kind)) throw fail("GR-103");
      limitNodes(p);
      limitEdges(p);
      // The node's regular inputs move to the effect with the same levels; sidechain stays on the node.
      const moved = p.graph.edges.filter((e) => e.to === node && !e.sidechain);
      p.graph.edges = p.graph.edges.filter((e) => !(e.to === node && !e.sidechain));
      const id = pushEffect(p, info, values, x, y);
      for (const edge of moved) {
        checkEdge(p, edge.from, id, false);
        p.graph.edges.push({ ...edge, to: id });
        moveEdgeLane(p, edge, edge.from, id);
      }
      checkEdge(p, id, node, false);
      p.graph.edges.push({ from: id, to: node, level: 1, sidechain: false, preFader: false });
      return id;
    });
  };
  h["graph.removeEffectKeepPath"] = ({ id }) => w.edit(t().mock.undo.removeEffect, (p) => {
    if (nodeOf(p, id).kind.type !== "effect") throw fail("GR-160", { id });
    const inputs = p.graph.edges.filter((e) => e.to === id && !e.sidechain);
    const outputs = p.graph.edges.filter((e) => e.from === id && !e.sidechain);
    dropNode(p, id);
    for (const input of inputs) {
      for (const output of outputs) {
        checkEdge(p, input.from, output.to, false);
        const level = clamp(input.level * output.level, 0, MAX_LEVEL);
        const same = p.graph.edges.find((e) => e.from === input.from && e.to === output.to && !e.sidechain);
        if (same) {
          // A parallel path already existed: the signals add up.
          same.level = Math.min(MAX_LEVEL, same.level + level);
        } else {
          limitEdges(p);
          p.graph.edges.push({ from: input.from, to: output.to, level, sidechain: false, preFader: input.preFader });
        }
      }
    }
  });
  h["graph.setOutput"] = ({ node, target }) => w.edit(t().mock.undo.output, (p) => {
    nodeOf(p, node);
    // Pre-fader sends stay (except a send to the new output itself: a pair has one connection).
    p.graph.edges = p.graph.edges.filter((e) => e.from !== node || e.sidechain || (e.preFader && e.to !== target));
    checkEdge(p, node, target, false);
    p.graph.edges.push({ from: node, to: target, level: 1, sidechain: false, preFader: false });
    dropStaleEdgeLanes(p);
  });
  return h;
}

