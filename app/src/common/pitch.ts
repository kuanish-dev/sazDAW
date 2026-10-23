import { clamp } from "./math";

/** Middle C (C4) is the default note: step-input pitch of a new track and the sampler's root note. */
export const MIDDLE_C = 60;

/** Piano roll pitch range (C1 … C7). */
export const LOW_PITCH = 24;
export const HIGH_PITCH = 96;

/** Velocity of a new note (MIDI, 1…127). */
export const DEFAULT_VELOCITY = 100;
export const MAX_VELOCITY = 127;
/** Velocity of a live note (computer keyboard, preview), fraction 0…1. */
export const LIVE_VELOCITY = 0.8;

/** General MIDI drum pitches: used by step input and part building. */
export const GM_DRUM_PITCHES: Readonly<Record<string, number>> = {
  kick: 36, rim: 37, snare: 38, clap: 39, hat: 42, tom_lo: 45, openhat: 46, tom_mid: 47, crash: 49, tom_hi: 50, ride: 51,
} as const;

const NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;
const BLACK = new Set([1, 3, 6, 8, 10]);

/** Pitch class name without octave: 0 is "C", 1 is "C#" … 11 is "B". */
export const pitchClassName = (pitchClass: number): string => NAMES[((pitchClass % 12) + 12) % 12]!;

export function pitchName(pitch: number): string {
  return `${NAMES[pitch % 12]}${Math.floor(pitch / 12) - 1}`;
}

export function isBlackKey(pitch: number): boolean {
  return BLACK.has(pitch % 12);
}

/** Pitch clamped to the editor's working range. */
export const clampPitch = (pitch: number): number => clamp(pitch, LOW_PITCH, HIGH_PITCH);
