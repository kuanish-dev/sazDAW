/** Buffer sizes offered in audio settings: powers of two. */
const POWERS = [64, 128, 256, 512, 1024, 2048, 4096] as const;

/** Recording latency limit, ms (same as the backend). */
export const MAX_RECORD_LATENCY_MS = 1000;

/** Allowed buffer sizes within the device range; unknown range — only "default". */
export function bufferChoices(range: [number, number] | null): number[] {
  if (!range) return [];
  return POWERS.filter((n) => n >= range[0] && n <= range[1]);
}

/** Latency of one buffer in milliseconds; null if the size is "default" or the sample rate is unknown. */
export function bufferLatencyMs(frames: number | null, sampleRate: number): number | null {
  if (frames === null || sampleRate <= 0) return null;
  return (frames / sampleRate) * 1000;
}

export const isRecordLatencyValid = (ms: number): boolean => ms >= 0 && ms <= MAX_RECORD_LATENCY_MS;

/**
 * The "Recording latency, ms" field: empty — `null` ("auto"), a valid number — itself,
 * anything else — `undefined` (input error).
 */
export function parseRecordLatency(text: string): number | null | undefined {
  if (text.trim() === "") return null;
  const ms = Number(text);
  return isRecordLatencyValid(ms) ? ms : undefined;
}

/** Buffer size from the select: empty — "default" (`null`). */
export const parseBufferFrames = (text: string): number | null => (text ? Number(text) : null);
