// Reverb graph (`ui/effects/ReverbGraph.tsx`): how fast the tail dies away, from the room size and damping, and the
// level of the reverb itself in the live sound. MIRRORS `crates/daw-effects/src/reverb.rs` (Freeverb: comb feedback
// `ROOM_OFFSET + room × ROOM_SCALE`, a one-pole damping lowpass `damp × DAMP_SCALE` inside each comb, the comb lengths
// `COMB_TUNING` at 44.1 kHz, the dry part `input × (1 − mix)`): a change there must be repeated here.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "./catalog";
import { READING_SECONDS } from "./liveLevels";

const COMB_TUNING = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const REFERENCE_RATE = 44_100;
const ROOM_OFFSET = 0.7;
const ROOM_SCALE = 0.28;
const DAMP_SCALE = 0.4;
/** Sample rate for drawing: the damping lowpass works per sample, so its effect on a frequency depends on the rate. */
const DRAW_RATE = 48_000;
/** The frequency of the "highs" edge of the tail: where damping is clearly heard. */
const HIGH_HZ = 5_000;
/** One pass through an average comb, seconds (the lengths scale with the rate, so this time does not). */
const PASS_SECONDS = COMB_TUNING.reduce((sum, n) => sum + n, 0) / COMB_TUNING.length / REFERENCE_RATE;

export interface ReverbSettings {
  room: number;
  damp: number;
  mix: number;
}

export const reverbSettings = (infos: ParamInfo[], values: Params): ReverbSettings | null => namedParams(infos, values, ["room", "damp", "mix"]);

/** Seconds the tail takes to fall by 60 dB at `freq` (RT60): every pass through a comb multiplies it by the comb's
 *  feedback and by the damping lowpass's gain at that frequency, `(1 − d) / |1 − d·e^(−jω)|` (1 for the lows). */
function decaySeconds(s: ReverbSettings, freq: number): number {
  const feedback = ROOM_OFFSET + s.room * ROOM_SCALE;
  const d = s.damp * DAMP_SCALE;
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const lowpass = (1 - d) / Math.sqrt(1 - 2 * d * Math.cos(w) + d * d);
  return (3 * PASS_SECONDS) / -Math.log10(feedback * lowpass);
}

/** RT60 of the lows (no damping) and of the highs (`HIGH_HZ`). */
export const reverbDecay = (s: ReverbSettings) => ({ low: decaySeconds(s, 0), high: decaySeconds(s, HIGH_HZ) });

/** The reverb's own sound in the newest stretch of a window (`READING_SECONDS`), RMS: the output minus its dry part,
 *  `input × (1 − mix)`, is exactly the mixed-in reverb (the dry sound and the reverb add up in `Reverb::process`). */
export function wetLevel(input: readonly number[], output: readonly number[], mix: number, sampleRate: number): number {
  const count = Math.min(input.length, output.length, Math.max(1, Math.round(sampleRate * READING_SECONDS)));
  let power = 0;
  for (let i = 1; i <= count; i++) {
    const wet = output[output.length - i]! - input[input.length - i]! * (1 - mix);
    power += wet * wet;
  }
  return count > 0 ? Math.sqrt(power / count) : 0;
}
