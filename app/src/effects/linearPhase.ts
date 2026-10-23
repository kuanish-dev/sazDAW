/** The kernel length, frames, at a sample rate. */
const kernelLength = (sampleRate: number) => (sampleRate <= 50_000 ? 4_096 : sampleRate <= 100_000 ? 8_192 : 16_384);

/** How long the linear-phase EQ delays the sound at `sampleRate`, ms. */
export const linearPhaseLatencyMs = (sampleRate: number) => ((kernelLength(sampleRate) * 1.5) / sampleRate) * 1000;
