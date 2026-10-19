// Delay graph (`effects/DelayGraph.tsx`): when the echoes of one sound come and how loud, from the delay's params and
// the tempo, and how the live input lights them up. MIRRORS `crates/daw-effects/src/delay.rs` (the `SYNC_QUARTERS`
// lengths, the time from the tempo or from milliseconds within `MAX_SECONDS`, the feedback per repeat, the ping-pong
// crossing that starts on the left) and `mix_over_dry` in `lib.rs`: a change there must be repeated here. The low and
// high cut in the delay's loop change the echoes' tone, not their level in the passband, so the taps leave them out.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import { clamp } from "../common/math";
import { toDb } from "../common/plot";

/** Lengths of the `sync` choices in quarter notes; the first is "Off" (the time comes from milliseconds). */
const SYNC_QUARTERS = [0, 2, 1, 1.5, 2 / 3, 0.5, 0.75, 1 / 3, 0.25, 0.375, 1 / 6, 0.125];
const MAX_SECONDS = 2;
/** The bars' scale runs from here to 0 dB; quieter echoes are not drawn. */
export const TAPS_FLOOR_DB = -48;
/** At most this many echoes: with a high feedback the later ones crowd together and say nothing new. */
const MAX_TAPS = 12;
/** More beat lines than this in the graph would be a blur: then none are drawn. */
const MAX_BEAT_LINES = 32;

/** Where an echo sounds: in both channels, or (ping-pong) on one side, starting on the left. */
export type TapSide = "both" | "left" | "right";

export interface Tap {
  seconds: number;
  db: number;
  side: TapSide;
}

export interface DelayTaps {
  /** Time between echoes, seconds. */
  step: number;
  /** The dry sound at 0 s, dB (`mix_over_dry` lowers it to half at full mix). */
  dryDb: number;
  taps: Tap[];
  /** How much time the graph shows: half a step past the last echo (at least two steps). */
  span: number;
  /** The `sync` choice: 0 when the time is in milliseconds, otherwise the note length it follows. */
  sync: number;
}

/** The echoes by the delay's param names at `tempo` (bpm); `null` when a param is missing. */
export function delayTaps(infos: ParamInfo[], values: Params, tempo: number): DelayTaps | null {
  const s = namedParams(infos, values, ["time", "feedback", "mix", "ping_pong", "sync"]);
  if (!s) return null;
  const sync = clamp(Math.trunc(s.sync), 0, SYNC_QUARTERS.length - 1);
  const seconds = sync === 0 ? s.time / 1000 : (60 / tempo) * SYNC_QUARTERS[sync]!;
  const step = clamp(seconds, 0.001, MAX_SECONDS);
  const pingPong = s.ping_pong >= 0.5;
  const taps: Tap[] = [];
  // Echo n comes n steps later, quieter by the feedback n − 1 times; ping-pong crosses the lines, so it changes sides.
  for (let n = 1; n <= MAX_TAPS; n++) {
    const db = toDb(s.mix * s.feedback ** (n - 1));
    if (db < TAPS_FLOOR_DB) break;
    taps.push({ seconds: n * step, db, side: pingPong ? (n % 2 === 1 ? "left" : "right") : "both" });
  }
  return { step, dryDb: toDb(1 - s.mix / 2), taps, span: (Math.max(taps.length, 2) + 0.5) * step, sync };
}

/** How loud the dry sound and each echo are now: their own level over the input level `levelAgo(seconds)` (dB) of the
 *  moment the sound that makes them came in. */
export function heardTaps(t: DelayTaps, levelAgo: (seconds: number) => number): { dry: number; taps: number[] } {
  return { dry: t.dryDb + levelAgo(0), taps: t.taps.map((tap) => tap.db + levelAgo(tap.seconds)) };
}

/** Times of the beats within `span` seconds at `tempo` (the dotted grid); none when they would be too dense to read. */
export function beatTimes(span: number, tempo: number): number[] {
  const beat = 60 / tempo;
  const count = Math.floor(span / beat);
  return count > MAX_BEAT_LINES ? [] : Array.from({ length: count }, (_, i) => (i + 1) * beat);
}
