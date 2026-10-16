// The song loop as in `daw-transport`: what playback wraps around now. The loop region wins; otherwise the whole song
// (0 → the end of the last clip rounded up to a whole bar) while the song loop is on and nothing records.
import { songEnd } from "../domain/clips";
import { barLengthAt, barStart } from "../domain/tempoMap";
import type { MockWorld } from "./world";

export function wrapRegion(w: MockWorld): [number, number] | null {
  if (w.project.loopRegion) return w.project.loopRegion;
  const end = songEnd(w.project.clips);
  // Armed note recording and an audio take hold the song loop, as the backend recorders do.
  if (!w.songLoop || w.armed || w.recordingAudio || end === 0) return null;
  const start = barStart(w.project.tempoMap, end);
  return [0, start >= end ? end : start + barLengthAt(w.project.tempoMap, start)];
}
