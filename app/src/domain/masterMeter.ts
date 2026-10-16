// Math for the live master meter (`ui/mixer/MasterMeters.tsx`): where loudness and the phase correlation sit on the
// panel's scales, how its numbers read, and the vectorscope's mid/side picture of the newest stereo frames (mono stands
// upright, one channel alone leans 45° towards its side, opposite channels lie flat).
import { clamp } from "./math";

/** The loudness bars' scale, LUFS: quieter than −54 hardly matters for a mix, 0 is full scale. */
export const LOUDNESS_FLOOR = -54;
export const LOUDNESS_TOP = 0;
/** Reference marks on the loudness bars: the usual streaming target and the EBU R128 broadcast target. */
export const LOUDNESS_TARGETS = [-14, -23];
/** Above this true peak the master leaves no headroom for a lossy codec: the usual ceiling for streaming, dBTP. */
export const TRUE_PEAK_CEILING = -1;
/** How much larger than full scale the vectorscope draws a quiet master at most: a whisper does not fill the picture. */
const MAX_VECTOR_GAIN = 8;

/** A loudness on the bars' scale, 0..1; nothing to show is 0. */
export function loudnessFraction(lufs: number | null): number {
  if (lufs === null) return 0;
  return clamp((lufs - LOUDNESS_FLOOR) / (LOUDNESS_TOP - LOUDNESS_FLOOR), 0, 1);
}

/** A correlation on its scale, 0..1: −1 at the left edge, 0 in the middle, +1 at the right. */
export function correlationFraction(correlation: number): number {
  return (clamp(correlation, -1, 1) + 1) / 2;
}

/** A level to one decimal with a real minus sign; a dash when there is nothing to show. */
export function formatLevel(value: number | null): string {
  if (value === null) return "—";
  const text = Math.abs(value).toFixed(1);
  return value < 0 && text !== "0.0" ? `−${text}` : text;
}

/** A correlation to two decimals, signed; a dash in silence. */
export function formatCorrelation(value: number | null): string {
  if (value === null) return "—";
  const text = Math.abs(value).toFixed(2);
  if (text === "0.00") return text;
  return `${value < 0 ? "−" : "+"}${text}`;
}

/**
 * The vectorscope picture of interleaved stereo frames: `[x, y]` pairs in −1..1, y up. x is the side, (R − L) / √2, and
 * y the mid, (L + R) / √2: mono stands upright and a channel alone leans 45° towards its side. Scaled so the loudest
 * frame reaches the edge — the shape tells the width, not the level — but at most `MAX_VECTOR_GAIN` times.
 */
export function vectorPoints(vector: number[]): number[] {
  const points: number[] = [];
  let peak = 0;
  for (let i = 0; i + 1 < vector.length; i += 2) {
    const left = vector[i]!;
    const right = vector[i + 1]!;
    const x = (right - left) * Math.SQRT1_2;
    const y = (left + right) * Math.SQRT1_2;
    points.push(x, y);
    peak = Math.max(peak, Math.abs(x), Math.abs(y));
  }
  if (peak === 0) return points;
  const gain = Math.min(MAX_VECTOR_GAIN, 1 / peak);
  return points.map((value) => value * gain);
}
