// Track and folder colors: a project palette name (`TRACK_COLORS`) → the on-screen shade (names: `t().controls.colors`).
// The same in both themes: a clip or strip is a colored plate with dark text. A track without its own color takes one by
// its number in the project — not its row, which changes when folders collapse — so the header, the mixer strip and the
// clips always agree.
import type { Clip, Id, Track, TrackColor } from "../../api/types";

export const TRACK_COLOR_HEX: Record<TrackColor, string> = {
  red: "#e5534b", orange: "#e8833a", amber: "#e0b43d", yellow: "#d8d24a", lime: "#9ccc4a", green: "#5cb85c",
  teal: "#46c2b5", cyan: "#4ac0df", blue: "#4a8fdf", indigo: "#6f78e0", violet: "#a070e0", pink: "#e6607e",
};

/** Colors by track number for tracks without their own color. */
const NUMBER_COLORS = ["#e8833a", "#4aa3df", "#7bc47f", "#c970d6", "#e0b43d", "#e6607e", "#46c2b5", "#8f86e8"];

/** Audio clips of a track without its own color. */
const AUDIO_CLIP_COLOR = "#4a6fa5";

const colorHex = (color: TrackColor | null, number: number): string =>
  color ? TRACK_COLOR_HEX[color] : NUMBER_COLORS[number % NUMBER_COLORS.length]!;

/** On-screen color of track `id`: its own palette color or the one for its number; `null` — no such track. */
export function trackColorOf(tracks: readonly Track[], id: Id): string | null {
  const number = tracks.findIndex((t) => t.id === id);
  return number < 0 ? null : colorHex(tracks[number]!.color ?? null, number);
}

/** Clip color: its track's color; without one, a shared color for audio clips and the track's number color for note clips. */
export function clipColorOf(tracks: readonly Track[], clip: Clip): string {
  const own = tracks.find((t) => t.id === clip.track)?.color ?? null;
  if (!own && clip.content.type === "audio") return AUDIO_CLIP_COLOR;
  return trackColorOf(tracks, clip.track) ?? AUDIO_CLIP_COLOR;
}
