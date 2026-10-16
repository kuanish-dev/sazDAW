// The `tick` stream from the backend (~30 times per second) and derived playback positions for drawing.
import { createEffect, createMemo, createSignal, onCleanup, untrack, type Accessor } from "solid-js";
import type { RpcClient } from "../api/rpc";
import type { Tick } from "../api/types";
import { playheadAt, rebase, type PlayheadBase } from "../domain/playhead";

const EMPTY_TICK: Tick = {
  state: "stopped", positionTicks: 0, cue: null, tempo: 120, signature: { numerator: 4, denominator: 4 }, wrap: null,
  songLoop: true, metronome: false, mono: false, automationWrite: "off", cpu: 0, latencyMs: 0, meters: [], reductions: [],
  record: { armed: false, capturing: false, takeNotes: 0, quantizeTicks: 0 },
  preview: null,
};

/**
 * Cursor position for drawing on every screen frame rather than once per backend tick (~30 Hz): between ticks it is
 * extrapolated by `domain/playhead`. `null` means the transport is stopped. Frames run only while audio plays. Subscribe
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
    // Read the shown position untracked: the effect changes it itself and must react only to the tick.
    base = rebase(current, performance.now(), untrack(position));
    setPosition(base.position);
    if (frame === 0 && base.ticksPerMs > 0) frame = requestAnimationFrame(loop);
  });

  onCleanup(() => cancelAnimationFrame(frame));
  return position;
}

export function createPlayback(rpc: RpcClient) {
  const [tick, setTick] = createSignal<Tick>(EMPTY_TICK);
  onCleanup(rpc.onTick(setTick));

  return {
    tick,
    /**
     * Positions for drawing: derived from `tick`, they do not fire while the value is unchanged. Canvases read these
     * rather than `tick()`: ticks always arrive (meters are live), and redrawing on every tick would redraw everything
     * even with the player stopped. `null` means the transport is stopped.
     */
    playhead: createMemo(() => (tick().state === "stopped" ? null : tick().positionTicks)),
    /** Sample audition position in file frames; `null` when nothing is playing. */
    previewPlayhead: createMemo(() => tick().preview),
    /** Same as `playhead`, but on every screen frame (extrapolated between ticks): for light elements like the timeline cursor. */
    smoothPlayhead: createSmoothPlayhead(tick),
  };
}
