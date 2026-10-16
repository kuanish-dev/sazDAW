// Dropping a sample onto the timeline with the mouse: pure functions for where the clip lands and where a new track goes.
import type { Clip, Id, TempoMap, Track } from "../api/types";
import { canPlace } from "./clips";
import { tickAfterFrames } from "./tempoMap";
import { isAudioTrack } from "./tracks";

/** What is dragged onto the timeline: a project sample or a file from disk (imported first). */
export type SampleSource = { kind: "sample"; id: Id } | { kind: "file"; path: string };

/** Length in ticks of an audio clip starting at tick `start`, via the tempo map (audio plays in real time; at least one tick). */
export function sampleLengthTicks(frameCount: number, sampleRate: number, map: TempoMap, start: number): number {
  return Math.max(1, Math.round(tickAfterFrames(map, start, frameCount, sampleRate) - start));
}

/** What a drop on existing track `track` does: `instrument` means the sound becomes its instrument (notes stay); otherwise a clip lands at `startTick`. */
export interface DropPlan {
  track: Id;
  startTick: number;
  instrument: boolean;
}

/**
 * Decision for the track under the cursor: instrument track - replace the instrument with the sound; audio track - a clip if there is room (edge overlap means crossfade);
 * occupied or no track - `null` (cannot drop). New tracks are created by the two zones below the last track, not by this function.
 */
export function planDrop(tracks: Track[], clips: Clip[], target: Id | null, startTick: number, lengthTicks: number): DropPlan | null {
  const track = tracks.find((t) => t.id === target);
  if (!track) return null;
  if (!isAudioTrack(track)) return { track: track.id, startTick, instrument: true };
  return canPlace(clips, track.id, startTick, lengthTicks, true) ? { track: track.id, startTick, instrument: false } : null;
}

/** What a drop on the "new track" zone creates: an instrument (sampler for notes) or a ready-made block (audio clip). */
export type NewTrackKind = "instrument" | "block";
