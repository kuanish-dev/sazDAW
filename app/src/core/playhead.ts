import { PPQ, type Tick } from "../api/types";

/** Anchor point: the position reported by the backend and the moment (ms, `performance.now()`) we received it. */
export interface PlayheadBase {
  position: number;
  at: number;
  /** Playhead speed in ticks per millisecond; `0` means stopped (paused). */
  ticksPerMs: number;
  /** What playback wraps around (the loop region or the song): the playhead waits at its end for the jump. */
  wrap: [number, number] | null;
}

/** How far (ms) a new backend position may lag behind the displayed one without moving the playhead back (tick delivery jitter). */
const BACK_TOLERANCE_MS = 80;

/**
 * New anchor from a backend tick. The playhead does not jump back because of a slightly late tick:
 * a backward move is accepted only if it exceeds `BACK_TOLERANCE_MS` (seek, a jump back at the loop or song end).
 */
export function rebase(tick: Tick, now: number, shown: number | null): PlayheadBase {
  const ticksPerMs = tick.state === "playing" ? (tick.tempo * PPQ) / 60_000 : 0;
  const lagging = shown !== null && shown > tick.positionTicks && shown - tick.positionTicks < BACK_TOLERANCE_MS * ticksPerMs;
  return { position: lagging ? shown : tick.positionTicks, at: now, ticksPerMs, wrap: tick.wrap };
}

/** Playhead position at `now`: anchor plus elapsed time; at the loop or song end it holds until the next backend tick. */
export function playheadAt(base: PlayheadBase, now: number): number {
  const position = base.position + (now - base.at) * base.ticksPerMs;
  return base.wrap && base.position < base.wrap[1] ? Math.min(position, base.wrap[1]) : position;
}
