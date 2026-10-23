import { CAPTURE_LEASE_SECONDS, CLIP_DBTP, type MasterReading, VECTOR_FRAMES } from "../api/types";
import { mockLiveSamples } from "./liveSound";
import type { MockWorld } from "./world";

/** The loudness of the mock master sits this far below its peak level: a mastered mix's crest. */
const CREST_DB = 9;
/** The true peak sits this far above the mock peak level: a little between the samples. */
const OVER_DB = 0.6;
/** The short-term loudness follows the momentary one this fast (per second): about a 3 s window. */
const SHORT_TERM_RATE = 0.35;
const GATE_LUFS = -70;
/** The mock correlation wobbles around a wide but mono-safe mix. */
const CORRELATION_BASE = 0.55;
const CORRELATION_SWING = 0.25;
const CORRELATION_PERIOD_MS = 1700;
/** How much the side signal (left minus right) is of the mid one in the vectorscope picture. */
const WIDTH = 0.35;
/** The side signal is the music a few frames earlier: decorrelated enough to open up the picture. */
const SIDE_DELAY = 9;

/** What the mock meter has gathered since the reset; `null` in the world before the first reading. */
export interface MeterState {
  /** When the integration began (ms), so `seconds` runs with the clock. */
  since: number;
  lastRead: number;
  /** When the measuring stops unless read again (ms). */
  until: number;
  shortTerm: number | null;
  /** Every momentary and short-term reading above the gate since the reset, for the integrated loudness and the range. */
  momentaries: number[];
  shortTerms: number[];
  maxPeak: number | null;
}

const fresh = (now: number): MeterState => ({
  since: now, lastRead: now, until: now, shortTerm: null, momentaries: [], shortTerms: [], maxPeak: null,
});

/** The reset button, and playback from the song start. */
export function resetMasterMeter(w: MockWorld) {
  if (w.meter) w.meter = { ...fresh(performance.now()), shortTerm: w.meter.shortTerm, until: w.meter.until };
}

export function masterMeterReading(w: MockWorld, sampleRate: number): MasterReading {
  const now = performance.now();
  if (!w.meter || now >= w.meter.until) w.meter = fresh(now);
  const meter = w.meter;
  meter.until = now + CAPTURE_LEASE_SECONDS * 1000;
  const dt = (now - meter.lastRead) / 1000;
  meter.lastRead = now;

  const master = w.project.graph.nodes.find((node) => node.kind.type === "master");
  const level = w.state === "playing" && master ? (w.peaks.get(master.id) ?? 0) : 0;
  const peak = level > 0 ? 20 * Math.log10(level) + OVER_DB : null;
  const momentary = peak !== null && peak - CREST_DB > GATE_LUFS ? peak - CREST_DB : null;
  if (momentary === null) meter.shortTerm = null;
  else meter.shortTerm = meter.shortTerm === null ? momentary : meter.shortTerm + (momentary - meter.shortTerm) * Math.min(1, dt * SHORT_TERM_RATE);
  if (momentary !== null) meter.momentaries.push(momentary);
  if (meter.shortTerm !== null) meter.shortTerms.push(meter.shortTerm);
  if (peak !== null) meter.maxPeak = Math.max(meter.maxPeak ?? peak, peak);

  return {
    momentaryLufs: round(momentary),
    shortTermLufs: round(meter.shortTerm),
    integratedLufs: round(powerMean(meter.momentaries)),
    rangeLu: round(range(meter.shortTerms)),
    truePeakDb: round(peak),
    maxTruePeakDb: round(meter.maxPeak),
    clipped: meter.maxPeak !== null && meter.maxPeak >= CLIP_DBTP,
    correlation: momentary === null ? null : CORRELATION_BASE + CORRELATION_SWING * Math.sin(now / CORRELATION_PERIOD_MS),
    seconds: (now - meter.since) / 1000,
    vector: vectorFrames(w, sampleRate),
    sampleRate,
  };
}

const round = (value: number | null) => (value === null ? null : Math.round(value * 10) / 10);

/** Loudness values averaged as powers, the way the integrated loudness is. */
function powerMean(values: number[]): number | null {
  if (values.length === 0) return null;
  const power = values.reduce((sum, value) => sum + 10 ** (value / 10), 0) / values.length;
  return 10 * Math.log10(power);
}

/** From the 10th to the 95th percentile, like the loudness range. */
function range(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const at = (p: number) => sorted[Math.round((sorted.length - 1) * p)]!;
  return at(0.95) - at(0.1);
}

/** The newest mock frames as stereo: the music in the middle and a delayed copy of it on the sides. */
function vectorFrames(w: MockWorld, sampleRate: number): number[] {
  const mid = mockLiveSamples(w, sampleRate, VECTOR_FRAMES + SIDE_DELAY);
  const frames: number[] = [];
  for (let i = SIDE_DELAY; i < mid.length; i++) {
    const side = WIDTH * mid[i - SIDE_DELAY]!;
    frames.push(mid[i]! + side, mid[i]! - side);
  }
  return frames;
}
