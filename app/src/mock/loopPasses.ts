// Loop passes of an audio take, as in `daw-audio-recorder/src/passes.rs`: playback that jumps back at the end of the
// loop region cuts one straight recording into passes. In seconds rather than frames: the mock has no audio.
import type { TempoMap } from "../api/types";
import { secondsAt } from "../domain/tempoMap";

export interface LoopPass {
  seconds: number;
  /** The tick its first moment belongs to: the take start for the first pass, the loop start for the others. */
  startTick: number;
  /** It played to the loop end; the last pass of a take stopped mid-loop did not. */
  reachedEnd: boolean;
}

/** A take of `seconds` started at `startTick`; playback that starts at or past the loop end never jumps: one pass. */
export function loopPasses(map: TempoMap, startTick: number, seconds: number, loop: [number, number] | null): LoopPass[] {
  const whole = [{ seconds, startTick, reachedEnd: false }];
  if (!loop || startTick >= loop[1]) return whole;
  const length = secondsAt(map, loop[1]) - secondsAt(map, loop[0]);
  if (length <= 0) return whole;
  const passes: LoopPass[] = [];
  let [from, tick, boundary] = [0, startTick, secondsAt(map, loop[1]) - secondsAt(map, startTick)];
  while (from < seconds) {
    const to = Math.min(boundary, seconds);
    if (to > from) passes.push({ seconds: to - from, startTick: tick, reachedEnd: boundary <= seconds });
    [from, tick, boundary] = [to, loop[0], boundary + length];
  }
  return passes;
}
