// Track folder actions: create, collapse, rename, recolor, move a track into or out of a folder,
// mute/solo the whole folder. Errors are shown as notifications.
import type { FolderEdit, Id } from "../api/types";
import { nextFolderName } from "./trackRows";
import type { Core } from "../core/core";

export interface Folders {
  /** New folder from tracks (named "Folder N"): they are placed consecutively at the topmost one's position. */
  create(tracks: Id[]): Promise<void>;
  /** Name, collapsed state, color; collapsing is shown immediately. */
  edit(id: Id, edit: FolderEdit): Promise<void>;
  /** Removes the folder; the tracks stay in place. */
  remove(id: Id): Promise<void>;
  /**
   * Places the track at `index` in folder `folder` (`null` = outside folders; `members` = the folder's new member order).
   * Entering a folder is one undo step; leaving a folder for another place is two (leave, reorder).
   */
  place(track: Id, folder: Id | null, index: number, members: Id[]): Promise<void>;
  /** Moves the track to the end of folder `folder` (leaving its previous folder). */
  moveInto(folder: Id, track: Id): Promise<void>;
  /** The track leaves its folder and stays in place; if it was the last track, the folder is removed. */
  takeOut(track: Id): Promise<void>;
  setMute(id: Id, on: boolean): Promise<void>;
  setSolo(id: Id, on: boolean): Promise<void>;
}

export function createFolders(core: Core): Folders {
  const { api } = core;

  /** Takes a track out of its folder (if it is the last track the folder is removed, since the backend rejects empty folders). */
  const leave = async (track: Id, folder: Id): Promise<boolean> => {
    const rest = core.copy.project.tracks.filter((t) => t.folder === folder && t.id !== track).map((t) => t.id);
    return core.calls.attempt(rest.length === 0 ? api.folders.remove(folder) : api.folders.setTracks(folder, rest));
  };

  return {
    create: (tracks) => core.calls.exec(api.folders.create(nextFolderName(core.copy.project.folders), tracks)),
    async edit(id, edit) {
      if (edit.collapsed !== undefined) {
        core.copy.patchProject((p) => {
          const folder = p.folders.find((f) => f.id === id);
          if (folder) folder.collapsed = edit.collapsed!;
        });
      }
      await core.calls.exec(api.folders.edit(id, edit));
    },
    remove: (id) => core.calls.exec(api.folders.remove(id)),
    async place(track, folder, index, members) {
      if (folder !== null) return core.calls.exec(api.folders.setTracks(folder, members));
      const current = core.copy.project.tracks.find((t) => t.id === track)?.folder ?? null;
      if (current !== null && !(await leave(track, current))) return;
      await core.calls.exec(api.tracks.move(track, index));
    },
    async moveInto(folder, track) {
      const members = core.copy.project.tracks.filter((t) => t.folder === folder && t.id !== track).map((t) => t.id);
      await core.calls.exec(api.folders.setTracks(folder, [...members, track]));
    },
    async takeOut(track) {
      const folder = core.copy.project.tracks.find((t) => t.id === track)?.folder;
      if (folder != null) await leave(track, folder);
    },
    setMute: (id, on) => core.calls.exec(api.folders.setMute(id, on)),
    setSolo: (id, on) => core.calls.exec(api.folders.setSolo(id, on)),
  };
}
