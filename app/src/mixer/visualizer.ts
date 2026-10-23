import { clamp } from "../common/math";

/** Band edges, Hz: below 30 Hz is felt rather than heard, above 16 kHz little music has energy. */
const LOW_HZ = 30;
const HIGH_HZ = 16_000;
/** The quietest level a band shows, dB below full scale; quieter reads as empty. */
const FLOOR_DB = -72;

/** In-place iterative radix-2 FFT; the length must be a power of two. */
function fft(re: Float32Array, im: Float32Array) {
  const n = re.length;
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    for (; j & bit; bit >>= 1) j ^= bit;
    j ^= bit;
    if (i < j) {
      const r = re[i]!;
      re[i] = re[j]!;
      re[j] = r;
      const m = im[i]!;
      im[i] = im[j]!;
      im[j] = m;
    }
  }
  for (let size = 2; size <= n; size <<= 1) {
    const half = size / 2;
    const stepRe = Math.cos((-2 * Math.PI) / size);
    const stepIm = Math.sin((-2 * Math.PI) / size);
    for (let start = 0; start < n; start += size) {
      let wRe = 1;
      let wIm = 0;
      for (let k = 0; k < half; k++) {
        const a = start + k;
        const b = a + half;
        const tRe = re[b]! * wRe - im[b]! * wIm;
        const tIm = re[b]! * wIm + im[b]! * wRe;
        re[b] = re[a]! - tRe;
        im[b] = im[a]! - tIm;
        re[a] = re[a]! + tRe;
        im[a] = im[a]! + tIm;
        const next = wRe * stepRe - wIm * stepIm;
        wIm = wRe * stepIm + wIm * stepRe;
        wRe = next;
      }
    }
  }
}

/** Amplitude of each frequency bin (the first half of the FFT) of `samples`, whose length is a power of two. Hann-windowed,
 *  so a sound between bins does not smear over the whole spectrum; a full-scale sine reads about 1. */
export function magnitudes(samples: Float32Array): Float32Array {
  const n = samples.length;
  const re = new Float32Array(n);
  const im = new Float32Array(n);
  for (let i = 0; i < n; i++) re[i] = samples[i]! * (0.5 - 0.5 * Math.cos((2 * Math.PI * i) / (n - 1)));
  fft(re, im);
  const out = new Float32Array(n / 2);
  for (let k = 0; k < out.length; k++) out[k] = (Math.hypot(re[k]!, im[k]!) * 4) / n;
  return out;
}

/** Amplitude → 0..1 on a dB scale from `FLOOR_DB` to full scale. */
function toUnit(amplitude: number): number {
  const db = 20 * Math.log10(amplitude);
  return Number.isFinite(db) ? clamp((db - FLOOR_DB) / -FLOOR_DB, 0, 1) : 0;
}

/** `count` log-spaced bands between 30 Hz and 16 kHz (or half the sample rate), each the loudest bin in it, 0..1. */
export function spectrumBands(bins: Float32Array, sampleRate: number, count: number): number[] {
  if (bins.length === 0 || sampleRate <= 0) return Array.from({ length: count }, () => 0);
  const binHz = sampleRate / (bins.length * 2);
  const high = Math.min(HIGH_HZ, sampleRate / 2);
  const edge = (band: number) => LOW_HZ * (high / LOW_HZ) ** (band / count);
  return Array.from({ length: count }, (_, band) => {
    const first = Math.min(bins.length - 1, Math.floor(edge(band) / binHz));
    const last = Math.min(bins.length - 1, Math.max(first, Math.ceil(edge(band + 1) / binHz) - 1));
    let peak = 0;
    for (let k = first; k <= last; k++) peak = Math.max(peak, bins[k]!);
    return toUnit(peak);
  });
}

/** Where to start an oscilloscope trace of `span` samples: at the first rising zero crossing that leaves room for it, so
 *  a steady tone draws in the same place every frame; the newest samples when there is no crossing (silence, noise). */
export function traceStart(samples: Float32Array, span: number): number {
  const latest = Math.max(0, samples.length - span);
  for (let i = 1; i <= latest; i++) {
    if (samples[i - 1]! < 0 && samples[i]! >= 0) return i;
  }
  return latest;
}

/** Like `traceStart`, but the latest rising zero crossing that still leaves room for `span` samples, so the trace shows
 *  the newest sound (the effect pictures); the newest samples when there is no crossing. */
export function latestTraceStart(samples: Float32Array, span: number): number {
  const latest = Math.max(0, samples.length - span);
  for (let i = latest; i >= 1; i--) {
    if (samples[i - 1]! < 0 && samples[i]! >= 0) return i;
  }
  return latest;
}
