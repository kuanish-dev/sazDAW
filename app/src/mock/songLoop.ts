import { songEnd } from "../tracks/clipRules";
import { barLengthAt, barStart } from "../common/tempoMap";
import type { MockWorld } from "./world";

export function wrapRegion(w: MockWorld): [number, number] | null {
  if (w.project.loopRegion) return w.project.loopRegion;
  const end = songEnd(w.project.clips);
  if (!w.songLoop || w.armed || w.recordingAudio || end === 0) return null;
  const start = barStart(w.project.tempoMap, end);
  return [0, start >= end ? end : start + barLengthAt(w.project.tempoMap, start)];
}
