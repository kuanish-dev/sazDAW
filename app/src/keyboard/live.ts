// "Live" notes on a track (computer keyboard, piano roll audition). These are not project edits:
// no notifications, but failures go to the log (console.error -> errorReporter).
import type { Id } from "../api/types";
import type { Core } from "../core/core";

export interface LiveNotes {
  noteOn(track: Id, pitch: number, velocity: number): void;
  noteOff(track: Id, pitch: number): void;
}

export function createLiveNotes(core: Core): LiveNotes {
  const { api } = core;
  return {
    noteOn: (track, pitch, velocity) => void api.live.noteOn(track, pitch, velocity).catch((error) => console.error("live.noteOn", error)),
    noteOff: (track, pitch) => void api.live.noteOff(track, pitch).catch((error) => console.error("live.noteOff", error)),
  };
}
