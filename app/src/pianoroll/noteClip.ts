// Note clip: its content, the clip bar, the visible length in the editor (with a spare bar) and where the playhead is
// inside looped content. A clip has its own even grid: a bar is the bar of the time signature in effect at the clip start
// (`clipBarTicks`); signature changes mid-clip do not cut it. The backend computes it the same way
// (`daw-project/src/note_rules.rs: fit_length`).
import { MAX_CLIP_LOOP_TICKS, type Clip, type Note, type NoteContent, type TempoMap } from "../api/types";
import { barLengthAt } from "../common/tempoMap";

/** A note clip: its content is notes (audio clips are not opened in the note editor). */
export type NoteClip = Clip & { content: { type: "notes" } & NoteContent };

export function asNoteClip(clip: Clip | null | undefined): NoteClip | null {
  return clip && clip.content.type === "notes" ? (clip as NoteClip) : null;
}

/** Clip bar in ticks: the bar of the time signature at the clip start. */
export const clipBarTicks = (map: TempoMap, clip: Clip): number => barLengthAt(map, clip.startTick);

/** A clip's notes live in the clip itself; audio clips have no notes. */
export function noteContentOf(clip: Clip | undefined): NoteContent | undefined {
  return clip?.content.type === "notes" ? clip.content : undefined;
}

export function clipNotes(clip: Clip | undefined): Note[] {
  return noteContentOf(clip)?.notes ?? [];
}

/**
 * A translucent spare bar (of length `bar`) after the end of auto-length content: it is inactive
 * while it has no notes, and placing a note there grows the content.
 */
export function ghostTicks(content: NoteContent | undefined, bar: number): number {
  if (!content?.autoLength) return 0;
  return content.loopTicks + bar <= MAX_CLIP_LOOP_TICKS ? bar : 0;
}

/** Length of the note editor's visible area: clip content plus the spare bar. */
export function viewTicks(clip: Clip | undefined, bar: number): number {
  const content = noteContentOf(clip);
  return (content?.loopTicks ?? bar) + ghostTicks(content, bar);
}

/**
 * Where in its looped content (`loop` ticks repeated from `start` for `length`) a clip plays at song tick `position`:
 * content ticks; `null` while the position is outside the clip. Note clips and automation patterns loop alike.
 */
export function playheadInLoop(start: number, length: number, loop: number, position: number): number | null {
  const local = position - start;
  if (local < 0 || local >= length) return null;
  return local % Math.max(1, loop);
}
