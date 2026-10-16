// Live note recording into the track's clip under the cursor: a recording lap is the clip length, a new lap replaces
// the previous ones (as in `daw-recorder`). No clip under the cursor creates a one-bar clip.
import type { Clip, Id, Project } from "../api/types";
import { overlapsOthers } from "../domain/clips";
import { clamp } from "../domain/math";
import { t } from "../i18n";
import { MAX_VELOCITY } from "../domain/pitch";
import { clipBarTicks } from "../domain/noteClip";
import { barLengthAt, barStart } from "../domain/tempoMap";
import { newClip } from "./clipRules";
import { contentOf, emptyNotes, fitLength, noteLimit } from "./noteOps";
import type { MockWorld } from "./world";

export function capturing(w: MockWorld) {
  return w.armed && w.state === "playing";
}

export function startTake(w: MockWorld) {
  w.takePasses.clear();
  w.takeRecorded = [];
  w.takeNotes = 0;
  w.takeOpen = false;
  w.heldNotes.clear();
}

function clipUnderCursor(p: Project, track: Id, tick: number): Clip | undefined {
  return p.clips.find((c) => c.track === track && c.content.type === "notes" && tick >= c.startTick && tick < c.startTick + c.lengthTicks);
}

/** Recording lap length: the clip, but no longer than the content that can hold notes. */
const lapLength = (clip: Clip) => Math.min(clip.lengthTicks, noteLimit(contentOf(clip)));

/** End of a take: held notes are written up to the current position. */
export function closeTake(w: MockWorld) {
  for (const [key, held] of w.heldNotes) {
    const [track, pitch] = key.split(":").map(Number) as [number, number];
    recordNote(w, track, pitch, held.tick, Math.max(w.position, held.tick), held.velocity);
  }
  w.heldNotes.clear();
  w.takeOpen = false;
}

/** The first note of a new lap erases what this take recorded in the clip on earlier laps. */
export function enterPass(w: MockWorld, track: Id, tick: number) {
  const clip = clipUnderCursor(w.project, track, tick);
  if (!clip) return;
  const pass = Math.floor((tick - clip.startTick) / lapLength(clip));
  if (pass <= (w.takePasses.get(clip.id) ?? 0)) return;
  w.takePasses.set(clip.id, pass);
  const stale = w.takeRecorded.filter((r) => r.clip === clip.id && r.pass < pass);
  if (stale.length === 0) return;
  openTakeHistory(w);
  const content = contentOf(clip);
  content.notes = content.notes.filter((n) => !stale.some((r) => r.id === n.id));
  w.takeRecorded = w.takeRecorded.filter((r) => !stale.includes(r));
  w.takeNotes = w.takeRecorded.length;
  fitLength(content, clipBarTicks(w.project.tempoMap, clip));
  w.touch();
}

function openTakeHistory(w: MockWorld) {
  if (w.takeOpen) return;
  w.undoStack.push({ label: t().mockEdits.history.record, project: JSON.stringify(w.project) });
  w.redoStack = [];
  w.takeOpen = true;
}

/** The clip under the cursor or a new one-bar clip (by the tempo map); `undefined` if the place is taken. */
function targetClip(w: MockWorld, track: Id, tick: number): Clip | undefined {
  const existing = clipUnderCursor(w.project, track, tick);
  if (existing) return existing;
  const start = barStart(w.project.tempoMap, tick);
  const bar = barLengthAt(w.project.tempoMap, start);
  if (overlapsOthers(w.project.clips, track, start, bar)) return undefined;
  const content = emptyNotes();
  fitLength(content, bar);
  const clip = newClip(w.id(), track, start, bar, content);
  w.project.clips.push(clip);
  return clip;
}

/** Adds a recorded note within the clip's lap; the whole take is one undo step. */
export function recordNote(w: MockWorld, track: Id, pitch: number, start: number, end: number, velocity: number) {
  if (!w.project.tracks.some((t) => t.id === track && t.instrument)) return;
  openTakeHistory(w);
  const clip = targetClip(w, track, start);
  if (!clip) return;
  const content = contentOf(clip);
  const loop = lapLength(clip);
  const unit = w.recordQuantize;
  const relative = start - clip.startTick;
  const position = ((relative % loop) + loop) % loop;
  let startTick = unit ? Math.round(position / unit) * unit : Math.round(position);
  if (startTick >= loop) startTick = 0;
  const raw = Math.max(0, end - start);
  const length = Math.min(unit ? Math.max(unit, Math.round(raw / unit) * unit) : Math.max(30, Math.round(raw)), loop - startTick);
  const id = content.nextNoteId++;
  content.notes.push({ id, pitch, startTick, lengthTicks: length, velocity: clamp(Math.round(velocity * MAX_VELOCITY), 1, MAX_VELOCITY) });
  w.takeRecorded.push({ pass: Math.floor(relative / loop), clip: clip.id, id });
  w.takeNotes = w.takeRecorded.length;
  fitLength(content, clipBarTicks(w.project.tempoMap, clip));
  w.dirty = true;
  w.touch();
}
