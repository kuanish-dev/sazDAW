export const MOCK_SAMPLE_RATE = 48_000;
export const MOCK_FRAME_COUNT = MOCK_SAMPLE_RATE * 4;
const HIT_EVERY = 0.5;

function hash(n: number): number {
  const x = Math.sin(n * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

/** Signal value at a frame: a kick on even beats, a noise snare on odd ones, a tone at the end. */
export function mockValue(frame: number): number {
  const t = frame / MOCK_SAMPLE_RATE;
  if (t >= 3.0) {
    const local = t - 3.0;
    return Math.sin(2 * Math.PI * 330 * t) * 0.45 * Math.exp(-local * 1.2);
  }
  const hit = Math.floor(t / HIT_EVERY);
  const local = t - hit * HIT_EVERY;
  if (hit % 2 === 0) {
    const freq = 50 + 160 * Math.exp(-local / 0.03);
    return Math.sin(2 * Math.PI * freq * local) * Math.exp(-local / 0.11) * 0.9;
  }
  return (hash(frame) * 2 - 1) * Math.exp(-local / 0.07) * 0.6;
}

/** Envelope of a range by buckets; up to 48 samples per bucket are enough for drawing. */
export function mockPeaks(from: number, to: number, buckets: number): [number, number][] {
  const end = Math.min(to, MOCK_FRAME_COUNT);
  const start = Math.min(from, end);
  const result: [number, number][] = [];
  for (let b = 0; b < buckets; b++) {
    const a = start + Math.floor(((end - start) * b) / buckets);
    const z = start + Math.floor(((end - start) * (b + 1)) / buckets);
    const step = Math.max(1, Math.floor((z - a) / 48));
    let low = 0;
    let high = 0;
    for (let f = a; f < Math.max(z, a + 1) && f < MOCK_FRAME_COUNT; f += step) {
      const v = mockValue(f);
      low = Math.min(low, v);
      high = Math.max(high, v);
    }
    result.push([low, high]);
  }
  return result;
}
