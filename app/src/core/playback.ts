import { createEffect, createMemo, createSignal, onCleanup, untrack, type Accessor } from "solid-js";
import type { RpcClient } from "../api/rpc";
import type { Tick } from "../api/types";
import { playheadAt, rebase, type PlayheadBase } from "./playhead";

const EMPTY_TICK: Readonly<Tick> = {
  state: "stopped", positionTicks: 0, cue: null, tempo: 120, signature: { numerator: 4, denominator: 4 }, wrap: null,
  songLoop: true, metronome: false, mono: false, automationWrite: "off", cpu: 0, latencyMs: 0, meters: [], reductions: [], bands: [],
  record: { armed: false, capturing: false, takeNotes: 0, quantizeTicks: 0 },
  preview: null,
} as const;

/**
 * Cursor position for drawing on every screen frame rather than once per backend tick (~30 Hz): between ticks it is
 * extrapolated by `core/playhead`. `null` means the transport is stopped. Frames run only while audio plays. Subscribe
 * only light things to it (an element with `transform`, the timeline's glow layer that draws only the clips under the
 * playhead, the transport's beat lights), not heavy canvases: `playhead` is enough for them.
 */
function createSmoothPlayhead(tick: Accessor<Tick>): Accessor<number | null> {
  const [position, setPosition] = createSignal<number | null>(null);
  let base: PlayheadBase | null = null;
  let frame = 0;

  const loop = (now: number) => {
    if (!base || base.ticksPerMs === 0) {
      frame = 0;
      return;
    }
    setPosition(playheadAt(base, now));
    frame = requestAnimationFrame(loop);
  };

  createEffect(() => {
    const current = tick();
    if (current.state === "stopped") {
      base = null;
      setPosition(null);
      return;
    }
    base = rebase(current, performance.now(), untrack(position));
    setPosition(base.position);
    if (frame === 0 && base.ticksPerMs > 0) frame = requestAnimationFrame(loop);
  });

  onCleanup(() => cancelAnimationFrame(frame));
  return position;
}

/** What the UI reads of playback. */
export interface PlaybackReads {
  /** The latest backend tick (~30 per second): position, transport state, meters, load. */
  tick(): Tick;
  /**
   * Position for drawing: derived from `tick`, it does not fire while the value is unchanged. Canvases read it rather
   * than `tick()`: ticks always arrive (meters are live), and redrawing on every tick would redraw everything even with
   * the player stopped. `null` means the transport is stopped.
   */
  playhead(): number | null;
  /** Sample audition position in file frames; `null` when nothing is playing. */
  previewPlayhead(): number | null;
  /** Same as `playhead`, but on every screen frame (extrapolated between ticks): for light elements like the timeline cursor. */
  smoothPlayhead(): number | null;
}

export function createPlayback(rpc: RpcClient): PlaybackReads {
  const [tick, setTick] = createSignal<Tick>(EMPTY_TICK);
  onCleanup(rpc.onTick(setTick));

  return {
    tick,
    playhead: createMemo(() => (tick().state === "stopped" ? null : tick().positionTicks)),
    previewPlayhead: createMemo(() => tick().preview),
    smoothPlayhead: createSmoothPlayhead(tick),
  };
}
