// Track folders: create, edit, remove, membership, mute/solo of all tracks, with the backend's checks and error codes.
import { TRACK_COLORS, type FolderEdit, type Id, type Project } from "../../api/types";
import { sourceOfTrack } from "../../graph/graphRules";
import { t } from "../../i18n";
import { fail } from "../errors";
import { checkFolderTracks, placeInFolder, topmostTrack } from "../folderRules";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const folderOf = (w: MockWorld, p: Project, id: Id) => w.must(p.folders.find((f) => f.id === id), fail("MK-011", { id }));

function checkedName(name: string): string {
  const trimmed = String(name ?? "").trim();
  if (!trimmed) throw fail("MK-112");
  return trimmed;
}

export function checkColor(color: string) {
  if (!(TRACK_COLORS as readonly string[]).includes(color)) {
    throw fail("TR-101", { color, colors: TRACK_COLORS.join(", ") });
  }
}

export function foldersHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const strips = (label: string, id: Id, flag: "mute" | "solo", on: boolean) => {
    folderOf(w, w.project, id);
    return w.edit(label, (p) => {
      for (const track of p.tracks.filter((t) => t.folder === id)) {
        const kind = sourceOfTrack(p.graph, track.id)?.kind;
        if (kind && (kind.type === "instrument" || kind.type === "audio")) kind[flag] = on;
      }
    });
  };

  h["folders.create"] = ({ name, tracks }: { name: string; tracks: Id[] }) => {
    const checked = checkedName(name);
    checkFolderTracks(w.project, tracks);
    return w.edit(t().mock.undo.createFolder, (p) => {
      const anchor = topmostTrack(p, tracks);
      const id = w.id(p);
      p.folders.push({ id, name: checked, collapsed: false });
      placeInFolder(p, id, tracks, anchor);
      return id;
    });
  };
  h["folders.edit"] = ({ id, ...edit }: { id: Id } & FolderEdit) => {
    folderOf(w, w.project, id);
    const name = edit.name === undefined ? undefined : checkedName(edit.name);
    if (edit.color) checkColor(edit.color);
    return w.edit(t().mock.undo.editFolder, (p) => {
      const folder = folderOf(w, p, id);
      if (name !== undefined) folder.name = name;
      if (edit.collapsed !== undefined) folder.collapsed = edit.collapsed;
      if (edit.color !== undefined) folder.color = edit.color;
    });
  };
  h["folders.remove"] = ({ id }: { id: Id }) => {
    folderOf(w, w.project, id);
    return w.edit(t().mock.undo.removeFolder, (p) => {
      p.folders = p.folders.filter((f) => f.id !== id);
      for (const track of p.tracks) if (track.folder === id) track.folder = null;
    });
  };
  h["folders.setTracks"] = ({ id, tracks }: { id: Id; tracks: Id[] }) => {
    folderOf(w, w.project, id);
    checkFolderTracks(w.project, tracks);
    return w.edit(t().mock.undo.folderTracks, (p) => {
      const staying = p.tracks.find((t) => t.folder === id && tracks.includes(t.id));
      const anchor = staying?.id ?? topmostTrack(p, tracks);
      for (const track of p.tracks) if (track.folder === id && !tracks.includes(track.id)) track.folder = null;
      placeInFolder(p, id, tracks, anchor);
    });
  };
  h["folders.setMute"] = ({ id, on }: { id: Id; on: boolean }) => strips(t().mock.undo.folderMute, id, "mute", on);
  h["folders.setSolo"] = ({ id, on }: { id: Id; on: boolean }) => strips(t().mock.undo.folderSolo, id, "solo", on);
  return h;
}
