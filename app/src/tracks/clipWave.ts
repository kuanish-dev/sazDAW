// Audio clip waveform on the timeline: from the whole-sample envelope, one [min, max] pair per clip pixel.
import { PPQ } from "../api/types";
import { clamp } from "../common/math";

/** Envelope of the whole sample: `peaks` evenly cover `frameCount` frames. */
export interface SamplePeaks {
  frameCount: number;
  sampleRate: number;
  peaks: [number, number][];
}

/** Envelope bins per second of audio, and limits on their count per sample. */
const BUCKETS_PER_SECOND = 200;
const MIN_BUCKETS = 256;
const MAX_BUCKETS = 8000;

/** How many envelope bins to request from the backend for a sample of this length: ~200 per second, within sane limits. */
export function bucketsFor(frameCount: number, sampleRate: number): number {
  return clamp(Math.ceil((frameCount / sampleRate) * BUCKETS_PER_SECOND), MIN_BUCKETS, MAX_BUCKETS);
}

/**
 * Clip waveform by column: a clip of `lengthTicks` plays the sample from frame `offsetFrames` at tempo `tempo`.
 * Columns past the end of the sample (clip longer than the audio) are silence `[0, 0]`.
 */
export function clipColumns(
  wave: SamplePeaks, offsetFrames: number, lengthTicks: number, tempo: number, columns: number,
): [number, number][] {
  const framesPerTick = (wave.sampleRate * 60) / (tempo * PPQ);
  const framesPerColumn = (lengthTicks * framesPerTick) / columns;
  const bucketFrames = wave.frameCount / wave.peaks.length;
  const result: [number, number][] = [];
  for (let c = 0; c < columns; c++) {
    const from = offsetFrames + c * framesPerColumn;
    const to = from + framesPerColumn;
    if (from >= wave.frameCount) {
      result.push([0, 0]);
      continue;
    }
    const first = Math.floor(from / bucketFrames);
    const last = Math.max(first, Math.min(wave.peaks.length - 1, Math.ceil(to / bucketFrames) - 1));
    let low = 0;
    let high = 0;
    for (let b = first; b <= last; b++) {
      const peak = wave.peaks[b]!;
      low = Math.min(low, peak[0]);
      high = Math.max(high, peak[1]);
    }
    result.push([low, high]);
  }
  return result;
}
