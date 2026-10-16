// Step input: sixteenth-note steps of the clip content, pitch rows, which steps are on and which is playing.
import { STEP_TICKS, type Note } from "../api/types";

export const stepAt = (tick: number): number => Math.floor(tick / STEP_TICKS);

/** Number of steps in the content (plus the spare bar, if any). */
export const stepTotal = (contentTicks: number, ghostTicks = 0): number => Math.floor((contentTicks + ghostTicks) / STEP_TICKS);

/** Rows: every pitch that has notes, plus the track's default pitch; highest on top. */
export function stepRows(notes: Note[], defaultPitch: number): number[] {
  return [...new Set([defaultPitch, ...notes.map((n) => n.pitch)])].sort((a, b) => b - a);
}

/** Notes starting at the step (at pitch `pitch`, if given). */
export function notesAtStep(notes: Note[], step: number, pitch?: number): Note[] {
  return notes.filter((n) => stepAt(n.startTick) === step && (pitch === undefined || n.pitch === pitch));
}

/** Whether the step is on at the pitch: a note of that pitch starts in it. */
export const stepOn = (notes: Note[], pitch: number, step: number): boolean => notesAtStep(notes, step, pitch).length > 0;

/** Groups of 4 steps (a beat) alternate color, as in a step sequencer. */
export const stepGroupOdd = (step: number): boolean => Math.floor(step / 4) % 2 === 1;

/** Which step is currently playing within the content (for highlighting), or `null`. */
export function activeStepOf(localTick: number | null, total: number): number | null {
  return localTick === null || total <= 0 ? null : stepAt(localTick) % total;
}
