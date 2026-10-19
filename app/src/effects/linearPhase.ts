// The linear-phase EQ's latency for its graph's note. MIRRORS `kernel_len` and `Convolver::latency` in
// `crates/daw-effects/src/linear_eq/`: a block of the kernel's length plus half the kernel, the kernel 4,096 frames up to
// 50 kHz, 8,192 up to 100 kHz and 16,384 above: a change there must be repeated here.

/** The kernel length, frames, at a sample rate. */
const kernelLength = (sampleRate: number) => (sampleRate <= 50_000 ? 4_096 : sampleRate <= 100_000 ? 8_192 : 16_384);

/** How long the linear-phase EQ delays the sound at `sampleRate`, ms. */
export const linearPhaseLatencyMs = (sampleRate: number) => ((kernelLength(sampleRate) * 1.5) / sampleRate) * 1000;
