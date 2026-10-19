// Project tracks: lookup, kind (instrument or audio), name and step pitch of a new track.
import type { Clip, Id, Project, Track } from "../api/types";
import { GM_DRUM_PITCHES, MIDDLE_C } from "../common/pitch";
import { STRING_PITCHES } from "../instruments/strings";
import { SYNTH_KIND } from "../instruments/synth";

/** Step-input pitch for the built-in synth: bass register (C3). */
const SYNTH_PITCH = 48;

export function trackById(project: Project, id: Id | null): Track | undefined {
  return id === null ? undefined : project.tracks.find((t) => t.id === id);
}

export function clipById(project: Project, id: Id | null): Clip | undefined {
  return id === null ? undefined : project.clips.find((c) => c.id === id);
}

/** An instrument track holds note clips; an audio track (`instrument: null`) holds audio clips. */
export const isAudioTrack = (track: Track): boolean => track.instrument === null;

export const firstAudioTrack = (project: Project): Track | undefined => project.tracks.find(isAudioTrack);

/** Tracks that play notes (a keyboard, a drum or chord part can target them). */
export const instrumentTracks = (project: Pick<Project, "tracks">): Track[] => project.tracks.filter((t) => !isAudioTrack(t));

/** Pitch that step input uses for a new track, by instrument kind (`null` means an audio track). */
export function defaultPitchFor(kind: string | null): number {
  if (kind === null) return MIDDLE_C;
  if (kind === SYNTH_KIND) return SYNTH_PITCH;
  return GM_DRUM_PITCHES[kind] ?? STRING_PITCHES[kind] ?? MIDDLE_C;
}

/** New track name: "Synth", or if `count` already exist, "Synth 2", "Synth 3"… */
export function nextTrackName(label: string, count: number): string {
  return count > 0 ? `${label} ${count + 1}` : label;
}
