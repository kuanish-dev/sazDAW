// The UI's copy of the project: project, status and kinds catalog come from the backend. On its change event the copy
// asks only for what changed since the revision it has (`project.changes`, taken in by `domain/projectChanges.ts`): a
// moved fader brings one graph node, not every note of the song. The first read and `reload` take the whole project.
import { batch, createSignal, onCleanup } from "solid-js";
import { createStore, produce, reconcile, unwrap } from "solid-js/store";
import type { DawApi } from "../api/api";
import type { RpcClient } from "../api/rpc";
import { type Catalog, DEFAULT_ROOM, type Project, type Status } from "../api/types";
import { applyChanges } from "../domain/projectChanges";

const EMPTY_PROJECT: Project = {
  name: "", tempoMap: { tempos: [{ tick: 0, bpm: 120, ramp: false }], signatures: [{ tick: 0, signature: { numerator: 4, denominator: 4 } }] },
  tracks: [], clips: [], loopRegion: null,
  graph: { nodes: [], edges: [] }, room: { ...DEFAULT_ROOM }, automation: [], samples: [], markers: [], folders: [], nextId: 1,
};

const EMPTY_STATUS: Status = { name: "", dirty: false, canUndo: false, canRedo: false, undoLabel: null, revision: 0, path: null, folder: null };

export interface ProjectSyncDeps {
  api: DawApi;
  rpc: RpcClient;
  /** Backend failure while reading: show it to the user. */
  onError(error: unknown): void;
}

/** What the UI reads of the project copy. */
export interface ProjectReads {
  readonly project: Project;
  status(): Status;
  catalog(): Catalog | null;
  /** The first project read is done: the views can be shown. */
  ready(): boolean;
  /** A project is open (saved in its own folder). */
  hasProject(): boolean;
}

export function createProjectSync(deps: ProjectSyncDeps) {
  const { api } = deps;
  const [project, setProject] = createStore<Project>(EMPTY_PROJECT);
  const [status, setStatus] = createSignal<Status>(EMPTY_STATUS);
  const [catalog, setCatalog] = createSignal<Catalog | null>(null);
  const [ready, setReady] = createSignal(false);

  /** The revision the copy is at; `null` before the first read (which therefore takes the whole project). */
  let revision: number | null = null;
  /** Hears each new copy in the same update batch (so the selection can give way to vanished items). */
  let loaded: ((project: Project) => void) | null = null;

  /** Takes in what changed since the copy's revision, or the whole project. */
  const read = async (whole: boolean) => {
    const [changes, nextStatus] = await Promise.all([api.project.changes(whole ? null : revision), api.project.status()]);
    revision = changes.revision;
    // The changed parts on top of the copy; reconciling skips the unchanged ones at once (they are the store's own objects).
    const next = applyChanges(unwrap(project), changes);
    batch(() => {
      setProject(reconcile(next, { key: "id", merge: true }));
      setStatus(nextStatus);
      loaded?.(next);
    });
  };

  // Reads take turns, so each starts from the revision the one before reached; a caller waits for its own. They are
  // small (only what changed), so there is nothing to merge.
  let reads: Promise<void> = Promise.resolve();
  const queue = (whole: boolean): Promise<void> => {
    reads = reads.then(() => read(whole)).catch(deps.onError);
    return reads;
  };
  const refresh = () => queue(false);
  /** Re-reads the whole project: after a refused call, whose value the interface may already show (`patch`). */
  const reload = () => queue(true);

  const refreshCatalog = async () => {
    setCatalog(await api.catalog.get());
  };

  const stop = deps.rpc.onProject(() => void refresh());
  onCleanup(stop);

  /** First read: catalog and project; the UI is shown after it. */
  const start = async () => {
    try {
      await refreshCatalog();
      await refresh();
    } catch (error) {
      deps.onError(error);
    }
    setReady(true);
  };

  return {
    project,
    status,
    catalog,
    ready,
    refresh,
    reload,
    refreshCatalog,
    start,
    hasProject: () => status().path !== null,
    /** Instant edit of the copy before the backend replies (sliders); the backend's value of the part arrives with the
     *  event, or with `reload` if the call is refused. */
    patchProject: (change: (p: Project) => void) => setProject(produce(change)),
    /** Who hears each new copy (one listener: the selection). */
    onLoaded: (listener: (project: Project) => void) => {
      loaded = listener;
    },
  };
}
