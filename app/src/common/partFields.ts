import { clamp } from "./math";

/** How many times a pattern or progression may repeat (the backend's limit). */
export const MAX_PART_REPEAT = 64;

/** A repeat count from a field: a whole number in `1…MAX_PART_REPEAT` (anything else — 1). */
export const parseRepeat = (text: string): number => clamp(Math.round(Number(text)) || 1, 1, MAX_PART_REPEAT);

/** The 0-based start bar from a field that counts bars from 1 (anything else — the first bar). */
export const parseStartBar = (text: string): number => Math.max(0, (Math.round(Number(text)) || 1) - 1);
