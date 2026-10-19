// Meter levels for all nodes (tracks, buses, master) and effect gain reduction: peaks from the `tick` stream with smooth decay,
// a peak-hold tick and a clip light (`mixer/mixer.ts: stepMeter`).
// One computation for the whole app, so meters in the timeline and the graph show the same thing.
import { createEffect, on } from "solid-js";
import { createStore } from "solid-js/store";
import type { Id, Tick } from "../api/types";
import { SILENT_LEVEL, decayPeak, isSilent, stepMeter, type MeterLevel } from "./mixer";

export type Levels = Record<Id, MeterLevel>;

/** Longest decay step, in seconds: after the tab was paused the meter does not drop to zero in one jump. */
const MAX_STEP_SECONDS = 0.2;

export function createLevels(tick: () => Tick): Levels {
  const [levels, setLevels] = createStore<Levels>({});
  let last = performance.now();
  // Runs on a new tick only (`on`): the body reads the store it writes, and every write puts new channel objects into
  // it, so a tracked read would wake this effect again after each write — an endless loop that overflows the stack.
  createEffect(on(tick, ({ meters }) => {
    const now = performance.now();
    const dt = Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
    last = now;
    for (const m of meters) {
      const prev = levels[m.node] ?? SILENT_LEVEL;
      // Silence that was already silence: writing it to the store would only wake subscribers.
      if (isSilent(prev.l) && isSilent(prev.r) && m.left === 0 && m.right === 0) continue;
      setLevels(m.node, { l: stepMeter(prev.l, m.left, dt), r: stepMeter(prev.r, m.right, dt) });
    }
  }));
  return levels;
}

export type Reductions = Record<Id, number>;

/** How fast the gain reduction meter returns to 0, in dB per second. */
const REDUCTION_FALL_DB = 30;

/** Gain reduction of effects (compressor, gate, limiter) in dB with the same hold-and-decay; an effect gone from the stream (bypassed, removed) falls to 0. */
export function createReductions(tick: () => Tick): Reductions {
  const [reductions, setReductions] = createStore<Reductions>({});
  let last = performance.now();
  // On a new tick only, like `createLevels`: the body reads the store it writes.
  createEffect(on(tick, (latest) => {
    const now = performance.now();
    const dt = Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
    last = now;
    const current = new Map(latest.reductions.map((r) => [r.node, r.db]));
    const nodes = new Set([...current.keys(), ...Object.keys(reductions).map(Number)]);
    for (const node of nodes) {
      const prev = reductions[node] ?? 0;
      const db = current.get(node) ?? 0;
      if (prev === 0 && db === 0) continue;
      setReductions(node, decayPeak(prev, db, dt, REDUCTION_FALL_DB));
    }
  }));
  return reductions;
}
