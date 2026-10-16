// Note audition: the note sounds on the clip's track while held (keys on the left or a dragged note).
import type { LiveActions } from "../../actions/live";
import type { Id } from "../../api/types";
import { LIVE_VELOCITY } from "../../domain/pitch";

export interface Audition {
  start(pitch: number): void;
  stop(): void;
}

export function createAudition(trackId: () => Id | undefined, live: LiveActions): Audition {
  let pitch = -1;
  const stop = () => {
    const track = trackId();
    if (track !== undefined && pitch >= 0) live.noteOff(track, pitch);
    pitch = -1;
  };
  return {
    stop,
    start(next) {
      const track = trackId();
      if (track === undefined || pitch === next) return;
      stop();
      pitch = next;
      live.noteOn(track, next, LIVE_VELOCITY);
    },
  };
}
