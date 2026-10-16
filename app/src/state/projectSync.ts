// The UI's copy of the project: project, status and kinds catalog come from the backend and are re-read on its event.
import { batch, createSignal, onCleanup } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import type { DawApi } from "../api/api";
import type { RpcClient } from "../api/rpc";
import { type Catalog, DEFAULT_ROOM, type Project, type Status } from "../api/types";

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
  /** Called in the same update batch as the new project (so the selection can give way to vanished items). */
  onLoaded(project: Project): void;
}

export function createProjectSync(deps: ProjectSyncDeps) {
  const { api } = deps;
  const [project, setProject] = createStore<Project>(EMPTY_PROJECT);
  const [status, setStatus] = createSignal<Status>(EMPTY_STATUS);
  const [catalog, setCatalog] = createSignal<Catalog | null>(null);
  const [ready, setReady] = createSignal(false);

  // Refreshes are coalesced: while a load is running, new requests only mark "once more",
  // and all callers await the same completion.
  let inflight: Promise<void> | null = null;
  let again = false;
  const load = async () => {
    do {
      again = false;
      const [nextProject, nextStatus] = await Promise.all([api.project.get(), api.project.status()]);
      batch(() => {
        setProject(reconcile(nextProject, { key: "id", merge: true }));
        setStatus(nextStatus);
        deps.onLoaded(nextProject);
      });
    } while (again);
  };
  const refresh = (): Promise<void> => {
    if (inflight) {
      again = true;
      return inflight;
    }
    inflight = load()
      .catch(deps.onError)
      .finally(() => {
        inflight = null;
        // An event may have arrived between leaving the `load` loop and here: do not lose the "once more".
        if (again) void refresh();
      });
    return inflight;
  };

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
    refreshCatalog,
    start,
    /** Whether a project is open (saved in its own folder). */
    hasProject: () => status().path !== null,
    /** Instant edit of the copy before the backend replies (sliders); the real project arrives with the event. */
    patch: (change: (p: Project) => void) => setProject(produce(change)),
  };
}
