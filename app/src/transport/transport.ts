// Transport (always plays the timeline): play/pause/stop, position and cue, the loop region, the song loop and the
// metronome. Errors are notifications.
// Tempo and meter are project edits (`transport/tempo.ts`); undo/redo is `history/history.ts`.
import type { Clip } from "../api/types";
import type { Core } from "../core/core";

export interface Transport {
  play(): Promise<void>;
  pause(): Promise<void>;
  stop(): Promise<void>;
  /** Space: playing -> pause, otherwise -> play. */
  togglePlay(): Promise<void>;
  /** Playhead position (a marker click). */
  seek(tick: number): Promise<void>;
  /** Click on the ruler: sets the start position (cue) that play and stop return to, and moves the playhead there. */
  cueAt(tick: number): Promise<void>;
  /** Removes the cue: pause resumes in place again and stop returns to the start. */
  clearCue(): Promise<void>;
  /** The region playback loops over (`null` — none). */
  setLoopRegion(region: [number, number] | null): Promise<void>;
  /** "Loop clip": the loop region becomes the clip's range. */
  loopClip(clip: Clip): Promise<void>;
  /** The song loop: at the song end playback jumps back to the start (the loop region wins; recording runs on). */
  setSongLoop(enabled: boolean): Promise<void>;
  setMetronome(enabled: boolean): Promise<void>;
}

export function createTransport(core: Core): Transport {
  const { api, calls } = core;
  const setLoopRegion: Transport["setLoopRegion"] = (region) => calls.exec(api.clips.setLoopRegion(region));
  return {
    play: () => calls.exec(api.transport.play()),
    pause: () => calls.exec(api.transport.pause()),
    stop: () => calls.exec(api.transport.stop()),
    togglePlay: () => calls.exec(core.playback.tick().state === "playing" ? api.transport.pause() : api.transport.play()),
    seek: (tick) => calls.exec(api.transport.seek(tick)),
    cueAt: (tick) => calls.exec(api.transport.setCue(tick).then(() => api.transport.seek(tick))),
    clearCue: () => calls.exec(api.transport.setCue(null)),
    setLoopRegion,
    loopClip: (clip) => setLoopRegion([clip.startTick, clip.startTick + clip.lengthTicks]),
    setSongLoop: (enabled) => calls.exec(api.transport.setSongLoop(enabled)),
    setMetronome: (enabled) => calls.exec(api.metronome.setEnabled(enabled)),
  };
}
