// Track folder rules of the mock, like `daw-project/src/folder_rules.rs`: a folder's tracks stand together at the
// topmost one, a folder without tracks does not survive.
import type { Id, Project } from "../api/types";
import { fail } from "./errors";

/** A folder's track list: not empty, no repeats, every track exists. */
export function checkFolderTracks(p: Project, tracks: Id[]) {
  if (!Array.isArray(tracks) || tracks.length === 0) throw fail("MK-113");
  const seen = new Set<Id>();
  for (const id of tracks) {
    if (!p.tracks.some((track) => track.id === id)) throw fail("TR-001", { id });
    if (seen.has(id)) throw fail("MK-114", { id });
    seen.add(id);
  }
}

/** Gathers each folder's tracks together at the topmost one and removes folders without tracks. */
export function gatherFolders(p: Project) {
  const known = new Set(p.folders.map((f) => f.id));
  const tracks = p.tracks.map((t) => (t.folder != null && !known.has(t.folder) ? { ...t, folder: null } : t));
  const done = new Set<Id>();
  p.tracks = [];
  for (const track of tracks) {
    if (track.folder == null) p.tracks.push(track);
    else if (!done.has(track.folder)) {
      done.add(track.folder);
      p.tracks.push(...tracks.filter((t) => t.folder === track.folder));
    }
  }
  p.folders = p.folders.filter((f) => done.has(f.id));
}

/** Puts `tracks` together into `folder` at the place of `anchor` (one of them), then gathers folders. */
export function placeInFolder(p: Project, folder: Id, tracks: Id[], anchor: Id) {
  const at = p.tracks.findIndex((t) => t.id === anchor);
  const before = p.tracks.slice(0, at).filter((t) => tracks.includes(t.id)).length;
  const block = tracks.map((id) => ({ ...p.tracks.find((t) => t.id === id)!, folder }));
  p.tracks = p.tracks.filter((t) => !tracks.includes(t.id));
  p.tracks.splice(Math.min(at - before, p.tracks.length), 0, ...block);
  gatherFolders(p);
}

export const topmostTrack = (p: Project, tracks: Id[]): Id => p.tracks.find((t) => tracks.includes(t.id))!.id;
