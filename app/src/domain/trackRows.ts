// Track list rows with folders and automation lanes — what is seen top to bottom in the headers and on the timeline. Pure
// functions shared by headers, canvas, mouse, sound drop and reordering: a folder row stands above its tracks, a collapsed
// folder hides them; a track's automation lanes stand right under it, lanes of no track (buses, master) in a block at the end.
import type { AutomationLane, Folder, Graph, Id, Track } from "../api/types";
import { t } from "../i18n";
import { sourceOfTrack } from "./graph";

export type TrackRow =
  | { type: "track"; track: Track; folder: Folder | null }
  | { type: "folder"; folder: Folder; tracks: Track[] }
  /** An automation lane; `track` — the track it belongs to (`null` — a bus or master lane). */
  | { type: "lane"; lane: AutomationLane; track: Track | null }
  /** The title row above the lanes of no track. */
  | { type: "busLanes" };

/** An automation lane with the track it belongs to (`domain/automation.ts` `ownedLanes`). */
export interface OwnedLane {
  lane: AutomationLane;
  track: Id | null;
}

/** Timeline rows in order: a track, or `null` for a row without track clips (folder, lane). */
export type Lanes = readonly (Track | null)[];

/** Rows in project track order: tracks of one folder are adjacent (the backend keeps them so), with the folder row above;
 * each track is followed by its lanes in project order, the lanes of no track close the list. */
export function trackRows(tracks: readonly Track[], folders: readonly Folder[], lanes: readonly OwnedLane[] = []): TrackRow[] {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const rows: TrackRow[] = [];
  const pushTrack = (track: Track, folder: Folder | null) => {
    rows.push({ type: "track", track, folder });
    for (const owned of lanes) if (owned.track === track.id) rows.push({ type: "lane", lane: owned.lane, track });
  };
  let i = 0;
  while (i < tracks.length) {
    const folder = byId.get(tracks[i]!.folder ?? -1);
    if (!folder) {
      pushTrack(tracks[i]!, null);
      i++;
      continue;
    }
    let end = i;
    while (end < tracks.length && tracks[end]!.folder === folder.id) end++;
    const members = tracks.slice(i, end);
    rows.push({ type: "folder", folder, tracks: members });
    if (!folder.collapsed) for (const track of members) pushTrack(track, folder);
    i = end;
  }
  const loose = lanes.filter((owned) => owned.track === null || !tracks.some((t) => t.id === owned.track));
  if (loose.length > 0) rows.push({ type: "busLanes" });
  for (const owned of loose) rows.push({ type: "lane", lane: owned.lane, track: null });
  return rows;
}

export const lanesOf = (rows: readonly TrackRow[]): Lanes => rows.map((r) => (r.type === "track" ? r.track : null));

/** Row of automation lane `lane`; `-1` — hidden (a collapsed folder) or gone. */
export const laneRowOf = (rows: readonly TrackRow[], lane: Id): number => rows.findIndex((r) => r.type === "lane" && r.lane.id === lane);

/** Track row on the timeline; `-1` — the track is hidden in a collapsed folder (or doesn't exist). */
export const laneOf = (lanes: Lanes, track: Id): number => lanes.findIndex((t) => t?.id === track);

/** Row of the collapsed folder hiding the track: a summary of its clips is drawn there; `-1` — not hidden. */
export function summaryRowOf(rows: readonly TrackRow[], track: Id): number {
  return rows.findIndex((r) => r.type === "folder" && r.folder.collapsed && r.tracks.some((t) => t.id === track));
}

/** First track in rows from `from` on (for a folder row — its first track), except `skip`. */
function firstTrackFrom(rows: readonly TrackRow[], from: number, skip: Id): Id | null {
  for (const row of rows.slice(from)) {
    const ids = row.type === "track" ? [row.track.id] : row.type === "folder" ? row.tracks.map((t) => t.id) : [];
    const found = ids.find((id) => id !== skip);
    if (found !== undefined) return found;
  }
  return null;
}

/** Folder that gap `slot` (between rows `slot - 1` and `slot`) leads into: a track of that folder (or one of its lanes)
 * is below it. */
function folderAtSlot(rows: readonly TrackRow[], slot: number): Id | null {
  const below = rows[slot];
  if (below?.type === "lane") return below.track?.folder ?? null;
  return below?.type === "track" && below.folder ? below.folder.id : null;
}

/** Where a dragged track lands: folder (`null` — outside folders) and place in the project track list. */
export interface TrackDrop {
  folder: Id | null;
  /** Index for `tracks.move`. */
  index: number;
  /** New members of `folder` in order (empty if `folder` is `null`). */
  members: Id[];
}

/**
 * Moves track `dragged` into row gap `slot`: a gap inside an expanded folder puts the track into the folder, a gap above
 * a folder row or below its last track — outside the folder. `null` — nothing changes.
 */
export function planTrackDrop(rows: readonly TrackRow[], tracks: readonly Track[], dragged: Id, slot: number): TrackDrop | null {
  const track = tracks.find((t) => t.id === dragged);
  if (!track) return null;
  const folder = folderAtSlot(rows, slot);
  const before = firstTrackFrom(rows, slot, dragged);
  const order = tracks.map((t) => t.id).filter((id) => id !== dragged);
  const at = before === null ? order.length : order.indexOf(before);
  order.splice(at, 0, dragged);
  const index = order.indexOf(dragged);
  if ((track.folder ?? null) === folder && tracks[index]?.id === dragged) return null;
  const members = folder === null ? [] : order.filter((id) => id === dragged || tracks.find((t) => t.id === id)?.folder === folder);
  return { folder, index, members };
}

/** New folder name: "Folder 1", "Folder 2"… */
export const nextFolderName = (folders: readonly Folder[]): string => t().tracks.folderName(folders.length + 1);

/**
 * A folder's M/S buttons: lit when the flag is set on every track of the folder (a folder is not a bus, so its M/S is
 * the flag on all its tracks' sources). A folder without playing tracks is neither.
 */
export function folderFlags(graph: Graph, tracks: readonly Track[]): { muted: boolean; soloed: boolean } {
  const strips = tracks.flatMap((track) => {
    const kind = sourceOfTrack(graph, track.id)?.kind;
    return kind && (kind.type === "instrument" || kind.type === "audio") ? [kind] : [];
  });
  return {
    muted: strips.length > 0 && strips.every((s) => s.mute),
    soloed: strips.length > 0 && strips.every((s) => s.solo),
  };
}
