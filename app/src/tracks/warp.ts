// Warping an audio clip to the tempo and its pitch: pure rules shared by the clip panel, timeline waveform and mock.
// They mirror the backend (`daw-contracts/src/warp.rs`, `daw-project/src/warp_rules.rs`).
import { PPQ, type Clip, type TempoMap } from "../api/types";
import { framesBetween, tickAfterFrames } from "../common/tempoMap";

export const MAX_PITCH_SEMITONES = 24;
export const MIN_BPM = 20;
export const MAX_BPM = 999;

/** Bar counts tried when guessing a loop's tempo from its length, and the plausible loop tempo. */
const LOOP_BARS = [1, 2, 4, 8, 16];
const GUESS_MIN = 70;
const GUESS_MAX = 180;
const TYPICAL_BPM = 120;
/** A number in the name without "bpm" counts as tempo only within these limits ("kick 01" is not a tempo). */
const NAME_MIN = 60;
const NAME_MAX = 200;

/** The tempo by which the clip measures audio: the source tempo if warped, otherwise the project tempo. */
export function clipTempo(clip: Clip, tempo: number): number {
  return clip.warp?.sourceBpm ?? tempo;
}

/**
 * How many frames of audio at `sampleRate` pass during the clip's first `ticks` ticks: if warped, by its source
 * tempo; otherwise by the tempo map from the clip start (audio plays in real time).
 */
export function ticksToSourceFrames(clip: Clip, ticks: number, map: TempoMap, sampleRate: number): number {
  if (clip.warp) return (ticks / PPQ) * (60 / clip.warp.sourceBpm) * sampleRate;
  return framesBetween(map, clip.startTick, clip.startTick + ticks, sampleRate);
}

export function sourceFramesToTicks(clip: Clip, frames: number, map: TempoMap, sampleRate: number): number {
  if (clip.warp) return (frames / sampleRate) * (clip.warp.sourceBpm / 60) * PPQ;
  return tickAfterFrames(map, clip.startTick, frames, sampleRate) - clip.startTick;
}

/** Source tempo: from the file name ("loop_128bpm", "Drums 140 Am"), otherwise from the length - a whole number of 4/4 bars at 70-180. */
export function guessBpm(name: string, frameCount: number, sampleRate: number): number | null {
  return bpmFromName(name) ?? bpmFromLength(frameCount, sampleRate);
}

function bpmFromName(name: string): number | null {
  const lower = name.toLowerCase();
  let standalone: number | null = null;
  for (const match of lower.matchAll(/\d+(?:\.\d+)?/g)) {
    const value = Number(match[0]);
    const start = match.index;
    const end = start + match[0].length;
    if (/^[ _-]*bpm/.test(lower.slice(end)) && value >= MIN_BPM && value <= MAX_BPM) return value;
    const before = start === 0 || !/[\p{L}\p{N}]/u.test(lower[start - 1]!);
    const after = end >= lower.length || !/[\p{L}\p{N}]/u.test(lower[end]!);
    if (standalone === null && before && after && value >= NAME_MIN && value <= NAME_MAX) standalone = value;
  }
  return standalone;
}

function bpmFromLength(frameCount: number, sampleRate: number): number | null {
  const seconds = frameCount / Math.max(1, sampleRate);
  if (seconds <= 0) return null;
  const fits = LOOP_BARS.map((bars) => (bars * 4 * 60) / seconds).filter((bpm) => bpm >= GUESS_MIN && bpm <= GUESS_MAX);
  if (fits.length === 0) return null;
  const distance = (bpm: number) => Math.abs(Math.log2(bpm / TYPICAL_BPM));
  const best = fits.reduce((a, b) => (distance(b) < distance(a) ? b : a));
  return Math.round(best * 1000) / 1000;
}

/** Pitch rounded to hundredths of a semitone (as the backend stores it). */
export function roundSemitones(semitones: number): number {
  return Math.round(semitones * 100) / 100;
}

/** Short warp and pitch tag for the clip label: "⟲ 128", "↕ +3". */
export function warpMarks(clip: Clip): string {
  const marks: string[] = [];
  if (clip.warp) marks.push(`⟲ ${formatBpm(clip.warp.sourceBpm)}`);
  if (clip.pitchSemitones !== 0) marks.push(`↕ ${clip.pitchSemitones > 0 ? "+" : ""}${clip.pitchSemitones}`);
  return marks.join(" ");
}

export function formatBpm(bpm: number): string {
  return String(Math.round(bpm * 100) / 100);
}
