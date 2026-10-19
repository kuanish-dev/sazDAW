// The live sound of graph nodes for the effect pictures: an editor's graph, or every effect along a signal path at
// once. One registry for the app: pictures ask to hear a node, the registry reads every node they show (at most
// `MAX_WATCHED_NODES`) in one batched call about 30 times a second (`audio.nodeWindows`). Reading is what keeps the
// engine capturing a node (`CAPTURE_LEASE_SECONDS`), so a node nobody shows any more is simply no longer read. It keeps a
// cursor per node, so each read carries only the frames that are new, keeps each node's latest `NODE_WINDOW_FRAMES` and
// hands every picture its node's whole window. While nothing is heard it does not poll, and the engine stops copying a
// few seconds later.
import { type Id, MAX_WATCHED_NODES, NODE_WINDOW_FRAMES, type NodeCursor, type NodeWindow } from "../api/types";
import type { Core } from "../core/core";
import { createPoller } from "../core/poller";

const POLL_MS = 33;
/** Longest step between readings, seconds: after a pause in polling the pictures do not jump to silence at once. */
const MAX_STEP_SECONDS = 0.2;

/** One reading of a node's sound. */
export interface NodeSound {
  /** What came into the node and what it sent on, mono, oldest first, paired frame by frame: its latest
   *  `NODE_WINDOW_FRAMES` (empty: silence, the node is not captured). */
  input: Float32Array;
  output: Float32Array;
  sampleRate: number;
  /** Seconds since the previous reading (at most `MAX_STEP_SECONDS`). */
  dt: number;
  /** When it was read, seconds on the picture clock (`performance.now`): the time of a history. */
  now: number;
}

export interface NodeSounds {
  /** Hears `node` until the returned function is called: `hear` gets every reading. A node beyond the first
   *  `MAX_WATCHED_NODES` heard is not captured and sounds silent. */
  listen(node: Id, hear: (sound: NodeSound) => void): () => void;
}

/** What the registry holds of one captured node. */
interface Kept {
  cursor: NodeCursor;
  input: Float32Array;
  output: Float32Array;
  sampleRate: number;
}

const SILENCE = new Float32Array(0);

/** `before` continued by `next`: their latest `NODE_WINDOW_FRAMES`. */
function append(before: Float32Array, next: Float32Array): Float32Array {
  if (next.length === 0) return before;
  const length = Math.min(NODE_WINDOW_FRAMES, before.length + next.length);
  if (next.length >= length) return next.subarray(next.length - length);
  const joined = new Float32Array(length);
  joined.set(before.subarray(before.length - (length - next.length)));
  joined.set(next, length - next.length);
  return joined;
}

export function createNodeSounds(core: Core): NodeSounds {
  const listeners = new Map<Id, Set<(sound: NodeSound) => void>>();
  const kept = new Map<Id, Kept>();
  let last = performance.now();

  /** The nodes to capture: the first ones asked for, as many as the engine watches. */
  const wanted = () => [...listeners.keys()].slice(0, MAX_WATCHED_NODES);

  /** Adds a reading to what is kept of its node: a window of the same watch continues it, another watch starts over. */
  const keep = (window: NodeWindow) => {
    const before = kept.get(window.node);
    const continues = before?.cursor.watch === window.watch;
    kept.set(window.node, {
      cursor: { node: window.node, watch: window.watch, end: window.end },
      input: append(continues ? before.input : SILENCE, window.input),
      output: append(continues ? before.output : SILENCE, window.output),
      sampleRate: window.sampleRate,
    });
  };

  const read = async () => {
    const nodes = wanted();
    const windows = await core.calls.run(core.api.audio.nodeWindows(nodes, nodes.flatMap((node) => kept.get(node)?.cursor ?? [])));
    if (!poller.running()) return;
    if (windows === undefined) {
      stop();
      return;
    }
    for (const window of windows) keep(window);
    for (const node of kept.keys()) if (!nodes.includes(node)) kept.delete(node);

    const now = performance.now();
    const dt = Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
    last = now;
    for (const [node, hears] of listeners) {
      const k = kept.get(node);
      const sound: NodeSound = { input: k?.input ?? SILENCE, output: k?.output ?? SILENCE, sampleRate: k?.sampleRate ?? 0, dt, now: now / 1000 };
      for (const hear of hears) hear(sound);
    }
  };

  const poller = createPoller(read, POLL_MS);

  /** Stops polling: nothing is heard, or a read failed (a toast said why; the next picture starts again). The engine stops
   *  capturing by itself once the leases run out. */
  const stop = () => {
    poller.stop();
    kept.clear();
  };

  return {
    listen(node, hear) {
      const hears = listeners.get(node) ?? new Set();
      hears.add(hear);
      listeners.set(node, hears);
      if (!poller.running()) {
        last = performance.now();
        poller.start();
      }
      return () => {
        hears.delete(hear);
        if (hears.size === 0) listeners.delete(node);
        if (listeners.size === 0 && poller.running()) stop();
      };
    },
  };
}
