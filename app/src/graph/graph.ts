// Audio graph actions: effects and buses, connections under graph rules (no cycles), faders of sources, buses
// and master, effect parameters, card layout. Errors (e.g. "would create a cycle") are notifications; sliders update immediately.
import type { GraphNode, Id, ParamTarget, Polarity } from "../api/types";
import { findNode, hasOutput, hasStrip, hasVolume, nextBusName } from "./graphRules";
import type { Core } from "../core/core";
import type { ParamTouch } from "../automation/paramTouch";

export interface ProcessingGraph {
  /** New unconnected effect; returns the node id, or `undefined` on failure. */
  addEffect(effect: string, x: number, y: number): Promise<Id | undefined>;
  /** New bus "Bus N", already connected to the master. */
  addBus(x: number, y: number): Promise<Id | undefined>;
  removeNode(id: Id): Promise<void>;
  renameNode(id: Id, name: string): Promise<void>;
  /** Moves a card: the position shows immediately; the backend merges a series of moves into one undo step. */
  moveNode(id: Id, x: number, y: number): Promise<void>;
  /** Moves cards to new positions one by one (so the backend merges them into one undo step). */
  arrange(positions: { id: Id; x: number; y: number }[]): Promise<void>;
  /** Creates a connection; repeating the same pair of the same kind changes its level. */
  connect(from: Id, to: Id, level: number, sidechain: boolean): Promise<boolean>;
  disconnect(from: Id, to: Id, sidechain: boolean): Promise<void>;
  /** Pre-fader (`true`) or post-fader (`false`) send for a regular connection from a source or bus. */
  setEdgePreFader(from: Id, to: Id, preFader: boolean): Promise<void>;
  /** Sidechain key of a compressor or gate: `source` replaces its key inputs and switches the key on; `null` removes it. */
  setSidechain(effect: Id, source: Id | null): Promise<void>;
  /** Volume of a source (track fader), bus or master. */
  setVolume(id: Id, volume: number): Promise<void>;
  /** Pan, mute and solo: on sources and buses. */
  setPan(id: Id, pan: number): Promise<void>;
  setMute(id: Id, mute: boolean): Promise<void>;
  setSolo(id: Id, solo: boolean): Promise<void>;
  /** The channel utility of a source or bus (acts before the volume and pan): polarity per side, width (automatable),
   * mono, fine time alignment in ms. Values show immediately; a knob gesture is one undo step. */
  setPolarity(id: Id, polarity: Polarity): Promise<void>;
  setWidth(id: Id, width: number): Promise<void>;
  setMono(id: Id, mono: boolean): Promise<void>;
  setDelay(id: Id, ms: number): Promise<void>;
  setEffectParam(id: Id, name: string, value: number): Promise<void>;
  setEffectBypass(id: Id, bypass: boolean): Promise<void>;
  /**
   * "+ Effect" at a node without breaking the signal path: right after it, or — for the master, which has no output —
   * right before it, routing all its inputs through the effect. Returns the effect's id or `undefined`.
   */
  insertEffectAt(node: Id, effect: string, x: number, y: number): Promise<Id | undefined>;
  /** Removes the effect and connects its inputs to its outputs. */
  removeEffectKeepPath(id: Id): Promise<void>;
  /**
   * Main output (the first regular post-fader connection from `from`) goes to `to`; sends are kept (mixer strip).
   * `setOutput` keeps only pre-fader sends, so post-fader sends are reconnected: several undo steps.
   */
  routeOutput(from: Id, to: Id): Promise<void>;
}

export function createProcessingGraph(core: Core, touch: ParamTouch): ProcessingGraph {
  const { api } = core;
  /** Shows the value immediately without waiting for the backend (sliders, knobs, cards), then asks the backend. */
  const showThen = (id: Id, change: (node: GraphNode) => void, send: () => Promise<unknown>) => {
    core.copy.patchProject((p) => {
      const node = p.graph.nodes.find((n) => n.id === id);
      if (node) change(node);
    });
    return core.calls.exec(send());
  };
  /** A control moved a parameter: live automation writing hears of it first, then the value shows and is sent. */
  const showMoved = (target: ParamTarget, value: number, id: Id, change: (node: GraphNode) => void, send: () => Promise<unknown>) => {
    touch.moved(target, value);
    return showThen(id, change, send);
  };
  const moveNode: ProcessingGraph["moveNode"] = (id, x, y) => showThen(id, (n) => Object.assign(n, { x, y }), () => api.graph.moveNode(id, x, y));

  return {
    addEffect: (effect, x, y) => core.calls.run(api.graph.addEffect(effect, x, y)),
    addBus: (x, y) => core.calls.run(api.graph.addBus(nextBusName(core.copy.project.graph), x, y)),
    removeNode: (id) => core.calls.exec(api.graph.removeNode(id)),
    renameNode: (id, name) => core.calls.exec(api.graph.renameNode(id, name)),
    moveNode,
    async arrange(positions) {
      for (const p of positions) await moveNode(p.id, p.x, p.y);
    },
    connect: (from, to, level, sidechain) => {
      // Connecting an existing pair again moves its level: that is a parameter move live writing may record.
      if (core.copy.project.graph.edges.some((e) => e.from === from && e.to === to && e.sidechain === sidechain)) {
        touch.moved({ type: "edgeLevel", from, to, sidechain }, level);
      }
      return core.calls.attempt(api.graph.connect(from, to, level, sidechain));
    },
    disconnect: (from, to, sidechain) => core.calls.exec(api.graph.disconnect(from, to, sidechain)),
    setEdgePreFader: (from, to, preFader) => core.calls.exec(api.graph.setEdgePreFader(from, to, preFader)),
    setSidechain: (effect, source) => core.calls.exec(api.graph.setSidechain(effect, source)),
    setVolume: (id, volume) => showMoved({ type: "nodeVolume", node: id }, volume, id, ({ kind }) => {
      if (hasVolume(kind)) kind.volume = volume;
    }, () => api.graph.setVolume(id, volume)),
    setPan: (id, pan) => showMoved({ type: "nodePan", node: id }, pan, id, ({ kind }) => {
      if (hasStrip(kind)) kind.pan = pan;
    }, () => api.graph.setPan(id, pan)),
    setMute: (id, mute) => showThen(id, ({ kind }) => {
      if (hasStrip(kind)) kind.mute = mute;
    }, () => api.graph.setMute(id, mute)),
    setSolo: (id, solo) => showThen(id, ({ kind }) => {
      if (hasStrip(kind)) kind.solo = solo;
    }, () => api.graph.setSolo(id, solo)),
    setPolarity: (id, polarity) => showThen(id, ({ kind }) => {
      if (hasStrip(kind)) kind.polarity = { ...polarity };
    }, () => api.graph.setPolarity(id, polarity)),
    setWidth: (id, width) => showMoved({ type: "nodeWidth", node: id }, width, id, ({ kind }) => {
      if (hasStrip(kind)) kind.width = width;
    }, () => api.graph.setWidth(id, width)),
    setMono: (id, mono) => showThen(id, ({ kind }) => {
      if (hasStrip(kind)) kind.mono = mono;
    }, () => api.graph.setMono(id, mono)),
    setDelay: (id, ms) => showThen(id, ({ kind }) => {
      if (hasStrip(kind)) kind.delayMs = ms;
    }, () => api.graph.setDelay(id, ms)),
    setEffectParam: (id, name, value) => showMoved({ type: "effectParam", node: id, param: name }, value, id, ({ kind }) => {
      if (kind.type === "effect") kind.params[name] = value;
    }, () => api.graph.setEffectParam(id, name, value)),
    setEffectBypass: (id, bypass) => core.calls.exec(api.graph.setEffectBypass(id, bypass)),
    insertEffectAt(node, effect, x, y) {
      const kind = findNode(core.copy.project.graph, node)?.kind;
      if (!kind) return Promise.resolve(undefined);
      return core.calls.run(hasOutput(kind) ? api.graph.insertEffect(node, effect, x, y) : api.graph.insertEffectBefore(node, effect, x, y));
    },
    removeEffectKeepPath: (id) => core.calls.exec(api.graph.removeEffectKeepPath(id)),
    async routeOutput(from, to) {
      // Copies, not references: the project is re-read after the first call.
      const post = core.copy.project.graph.edges.filter((e) => e.from === from && !e.sidechain && !e.preFader).map((e) => ({ ...e }));
      const sends = post.slice(1).filter((e) => e.to !== to);
      if (!(await core.calls.attempt(api.graph.setOutput(from, to)))) return;
      for (const send of sends) {
        if (!(await core.calls.attempt(api.graph.connect(from, send.to, send.level, false)))) return;
      }
    },
  };
}
