import type { HistorySteps, Project, ProjectChanges, Status } from "../../api/types";
import { pathForFormat } from "../../files/exportAudio";
import { t } from "../../i18n";
import { emptyProject } from "../demo";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const stem = (path: string) => (path.split("/").pop() ?? "").replace(/\.[^.]+$/, "") || t().mock.projectName;

/** The project status, as `project.status` and the feed's updates bring it. */
export function projectStatus(w: MockWorld): Status {
  return {
    name: w.project.name, dirty: w.dirty, canUndo: w.undoStack.length > 0, canRedo: w.redoStack.length > 0,
    undoLabel: w.undoStack.at(-1)?.label ?? null, path: w.path, folder: w.folder(),
  };
}

/** The whole project as changes: the mock's feed always sends it (which the contract allows), the backend only what
 *  changed (`daw-rpc/src/feed.rs`). */
export function wholeChanges(w: MockWorld): ProjectChanges {
  const p = w.clone(w.project);
  const list = <T extends { id: number }>(items: T[]) => ({ ids: items.map((item) => item.id), changed: items });
  return {
    name: p.name, tempoMap: p.tempoMap, tracks: list(p.tracks), clips: list(p.clips), loopRegion: p.loopRegion,
    nodes: list(p.graph.nodes), edges: p.graph.edges, room: p.room, automation: list(p.automation),
    samples: list(p.samples), markers: list(p.markers), folders: list(p.folders), key: p.key,
    midiMappings: p.midiMappings ?? [], nextId: p.nextId,
  };
}

export function projectHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["project.get"] = () => w.clone(w.project);
  h["project.status"] = () => projectStatus(w);
  h["catalog.get"] = () => w.catalog;
  h["project.new"] = () => { w.install(emptyProject(), null, false); };
  h["project.create"] = ({ path }) => {
    if (w.saved.has(path)) throw fail("PJ-110", { path });
    const fresh = emptyProject();
    fresh.name = stem(path);
    w.saved.set(path, JSON.stringify(fresh));
    w.install(fresh, path, false);
    w.remember(path);
  };
  h["project.save"] = ({ path }) => {
    if (path !== w.path) w.project.name = stem(path);
    w.saved.set(path, JSON.stringify(w.project));
    w.path = path;
    w.dirty = false;
    w.remember(path);
    w.touch();
  };
  h["project.load"] = ({ path }) => {
    const text = w.saved.get(path);
    if (!text) throw fail("PJ-472", { path, details: "No such file or directory (os error 2)" });
    w.install(JSON.parse(text) as Project, path, false);
    w.remember(path);
  };
  h["project.recent"] = () => w.recent.filter((p) => w.saved.has(p)).map((path) => ({ path, name: stem(path) }));
  h["project.forgetRecent"] = ({ path }) => { w.recent = w.recent.filter((p) => p !== path); };
  h["project.autosave"] = () => (w.autosave && w.path ? `${w.folder()}/autosave/autosave.dawproj` : null);
  h["project.restoreAutosave"] = () => {
    if (!w.autosave) throw fail("PJ-070");
    w.install(JSON.parse(w.autosave) as Project, w.path, true);
    w.autosave = null;
  };
  h["project.discardAutosave"] = () => { w.autosave = null; };
  h["project.takeWarnings"] = () => [];
  h["project.export"] = ({ path, stems, format, bitDepth, normalize, range }) => {
    if (format === "flac" && bitDepth === "float32") throw fail("EX-551");
    if (range === "loop" && !w.project.loopRegion) throw fail("EX-353");
    const file = pathForFormat(path, format ?? "wav");
    const stem = file.replace(/(\.[^.]+)$/, `_3_${t().mock.demo.drums}$1`);
    const loudnessLufs = normalize ? Math.min(normalize.lufs, (normalize.truePeakDb ?? -1) - 6) : -18.3;
    return { files: stems ? [file, stem] : [file], frames: 441000, loudnessLufs, peakDb: loudnessLufs + 6 };
  };
  h["history.undo"] = () => w.step(w.undoStack, w.redoStack);
  h["history.redo"] = () => w.step(w.redoStack, w.undoStack);
  h["history.list"] = (): HistorySteps => ({
    done: w.undoStack.map((entry) => entry.label),
    undone: w.redoStack.map((entry) => entry.label).reverse(),
  });
  h["history.checkpoint"] = () => w.undoStack.length;
  h["history.squash"] = ({ since, label }: { since: number; label: string }) => {
    const first = w.undoStack[since];
    if (!first) return false;
    w.undoStack.splice(since, w.undoStack.length - since, { label, project: first.project });
    w.touch();
    return true;
  };
  return h;
}
