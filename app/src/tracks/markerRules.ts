// Song arrangement: pure marker rules — the neighbouring marker for jumps and the name of a new marker.
import type { Marker } from "../api/types";
import { t } from "../i18n";

/** "Section 1", "Section 2"…: one more than the number of markers (a repeated name does no harm). */
export function nextMarkerName(markers: readonly Marker[]): string {
  return t().tempo.markerName(markers.length + 1);
}

/**
 * The marker to jump to from `tick`: `-1` is the nearest to the left (one standing almost at the position is skipped,
 * otherwise pressing again during playback would never go further), `1` is the nearest to the right. Markers are sorted by tick.
 */
export function neighbourMarker(markers: readonly Marker[], tick: number, direction: -1 | 1, slackTicks: number): Marker | undefined {
  if (direction > 0) return markers.find((m) => m.tick > tick);
  return [...markers].reverse().find((m) => m.tick < tick - slackTicks);
}
