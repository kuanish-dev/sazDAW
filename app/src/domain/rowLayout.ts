// Row geometry of the track list: every row has its own height — a track its own (`Track.height`, set by dragging the
// bottom edge of its header) or the interface setting (Appearance → track height), folder and automation lane rows the
// setting. Headers, the timeline canvas, the mouse, sound drops and reordering all place rows through this, so they stay
// aligned.
import type { Track } from "../api/types";
import { clamp } from "./math";
import type { TrackRow } from "./trackRows";

/** Bounds of a track's own height, pixels (the backend's `MIN_TRACK_HEIGHT`/`MAX_TRACK_HEIGHT`). */
export const MIN_TRACK_HEIGHT = 36;
export const MAX_TRACK_HEIGHT = 400;

/** Where the rows are: `tops[i]` is the top of row `i`; the last entry is the bottom of the last row. */
export interface RowLayout {
  readonly tops: readonly number[];
}

export const trackHeight = (track: Track, fallback: number): number => track.height ?? fallback;

export function rowLayout(rows: readonly TrackRow[], fallback: number): RowLayout {
  const tops = [0];
  for (const row of rows) tops.push((tops.at(-1) ?? 0) + (row.type === "track" ? trackHeight(row.track, fallback) : fallback));
  return { tops };
}

export const rowCount = (layout: RowLayout): number => layout.tops.length - 1;

export const rowTop = (row: number, layout: RowLayout): number => layout.tops[clamp(row, 0, rowCount(layout))] ?? 0;

export const rowHeight = (row: number, layout: RowLayout): number => rowTop(row + 1, layout) - rowTop(row, layout);

export const timelineHeight = (layout: RowLayout): number => rowTop(rowCount(layout), layout);

/** Row at a vertical coordinate; `null` above the rows or in the empty space below them. */
export function rowAt(y: number, layout: RowLayout): number | null {
  if (y < 0 || y >= timelineHeight(layout)) return null;
  let row = 0;
  while (row + 1 < rowCount(layout) && rowTop(row + 1, layout) <= y) row++;
  return row;
}

/** Row for dragging: outside the list, the first or the last row. */
export function clampRow(y: number, layout: RowLayout): number {
  const count = rowCount(layout);
  return clamp(rowAt(clamp(y, 0, timelineHeight(layout) - 1), layout) ?? 0, 0, count - 1);
}

/** Insertion slot between rows (0 is above the first, the row count below the last): the row edge nearest to `y`. */
export function insertSlot(y: number, layout: RowLayout): number {
  let nearest = 0;
  layout.tops.forEach((top, slot) => {
    if (Math.abs(top - y) < Math.abs(rowTop(nearest, layout) - y)) nearest = slot;
  });
  return nearest;
}
