import { batch, createSignal, onCleanup } from "solid-js";
import { createStore, produce, reconcile, unwrap } from "solid-js/store";
import type { DawApi } from "../api/api";
import type { RpcClient } from "../api/rpc";
import { type Catalog, DEFAULT_ROOM, type Project, type ProjectUpdate, type Status } from "../api/types";
import { applyChanges } from "./projectChanges";

const EMPTY_PROJECT: Readonly<Project> = {
  name: "", tempoMap: { tempos: [{ tick: 0, bpm: 120, ramp: false }], signatures: [{ tick: 0, signature: { numerator: 4, denominator: 4 } }] },
  tracks: [], clips: [], loopRegion: null,
  graph: { nodes: [], edges: [] }, room: { ...DEFAULT_ROOM }, automation: [], samples: [], markers: [], folders: [], key: null,
  nextId: 1,
} as const;

const EMPTY_STATUS: Readonly<Status> = { name: "", dirty: false, canUndo: false, canRedo: false, undoLabel: null, path: null, folder: null } as const;

export interface ProjectSyncDeps {
  api: DawApi;
  rpc: RpcClient;
  /** Backend failure while reading the catalog: show it to the user. */
  onError(error: unknown): void;
}

/** What the UI reads of the project copy. */
export interface ProjectReads {
  readonly project: Project;
  status(): Status;
  catalog(): Catalog | null;
  /** The first whole project came: the views can be shown. */
  ready(): boolean;
  /** A project is open (saved in its own folder). */
  hasProject(): boolean;
}

/** The project copy as a role uses it: what the UI reads, instant display before the backend replies, re-reads, and
 *  hearing each new copy. */
export type ProjectCopy = ProjectReads & {
  /** Asks for the whole project again (after a refused call: its value may already be shown by `patchProject`). */
  reload(): Promise<void>;
  /** Re-reads the kinds catalog (a plugin scan adds new effects to it). */
  refreshCatalog(): Promise<void>;
  /**
   * Shows a value in the UI immediately, before the backend replies (sliders, knobs, a finished drag). The backend's
   * value of the part arrives with the next update; a refused call asks for the whole project (`reload`), so a refused
   * value does not stay on screen. Only roles change the copy, never a component.
   */
  patchProject(change: (project: Project) => void): void;
  /** Hears each new copy in the same update batch as it (so the selection can give way to vanished items); returns
   *  the function that stops hearing. */
  onLoaded(listener: (project: Project) => void): () => void;
  /** Resolves once the copy has what `has` looks for, at once if it already does: what a call just made comes with
   *  the feed's next update. */
  whenHas(has: (project: Project) => boolean): Promise<void>;
};

export function createProjectSync(deps: ProjectSyncDeps) {
  const { api, rpc } = deps;
  const [project, setProject] = createStore<Project>(EMPTY_PROJECT);
  const [status, setStatus] = createSignal<Status>(EMPTY_STATUS);
  const [catalog, setCatalog] = createSignal<Catalog | null>(null);
  const [ready, setReady] = createSignal(false);
  const listeners = new Set<(project: Project) => void>();

  /** Whether the copy is a whole project yet: changes before that have nothing to go on top of. */
  let whole = false;
  let firstWhole: () => void = () => undefined;
  const firstWholeCame = new Promise<void>((resolve) => {
    firstWhole = resolve;
  });

  const hear = (listener: (project: Project) => void) => {
    listeners.add(listener);
    return () => void listeners.delete(listener);
  };

  const take = (update: ProjectUpdate) => {
    if (!update.whole && !whole) return;
    whole = true;
    const next = applyChanges(unwrap(project), update.changes);
    batch(() => {
      setProject(reconcile(next, { key: "id", merge: true }));
      setStatus(update.status);
      for (const listener of listeners) listener(next);
    });
    firstWhole();
  };
  onCleanup(rpc.onProject(take));

  const refreshCatalog = async () => {
    setCatalog(await api.catalog.get());
  };

  /** First read: the catalog, and the first whole project from the feed; the UI is shown after them. */
  const start = async () => {
    try {
      await refreshCatalog();
    } catch (error) {
      deps.onError(error);
    }
    await firstWholeCame;
    setReady(true);
  };

  return {
    project,
    status,
    catalog,
    ready,
    reload: () => rpc.askWholeProject(),
    refreshCatalog,
    start,
    hasProject: () => status().path !== null,
    /** Instant edit of the copy before the backend replies (sliders); the backend's value of the part arrives with the
     *  next update, or with the whole project if the call is refused. */
    patchProject: (change: (p: Project) => void) => setProject(produce(change)),
    onLoaded: hear,
    whenHas(has: (project: Project) => boolean) {
      if (has(unwrap(project))) return Promise.resolve();
      return new Promise<void>((resolve) => {
        const stop = hear((next) => {
          if (!has(next)) return;
          stop();
          resolve();
        });
      });
    },
  };
}
