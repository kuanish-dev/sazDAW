// The shared core of the interface: what every role asks for its work — the backend (`api`, system dialogs, the window
// shell), the project copy (`projectSync.ts`), how a call runs and tells the user (`calls.ts`) and the playback tick
// (`playback.ts`). A role holds the core and asks it, the way the entities on the backend hold the project core. The UI
// never gets the core itself, only what it may read of it (`CoreReads`), so no component calls the backend.
import type { DawApi } from "../api/api";
import { createApi } from "../api/api";
import type { SystemDialogs } from "../api/dialogs";
import type { RpcClient } from "../api/rpc";
import type { AppShell } from "../api/shell";
import { type Calls, createCalls } from "./calls";
import { createToasts, type ToastReads } from "./notifications";
import { createPlayback, type PlaybackReads } from "./playback";
import { createProjectSync, type ProjectCopy, type ProjectReads } from "./projectSync";

/** What lies below: the backend, system dialogs and the window shell (zoom and app version). */
export interface Backend {
  readonly api: DawApi;
  readonly dialogs: SystemDialogs;
  readonly shell: AppShell;
}

/** What a role gets: the backend, the project copy, how calls run and the playback position. */
export type Core = Backend & ProjectCopy & Calls & PlaybackReads;

/** What the UI reads of the core: the project copy, playback, notifications and the long-operation label. */
export type CoreReads = ProjectReads & PlaybackReads & ToastReads & { busy(): string | null };

export function createCore(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell) {
  const api = createApi(rpc);
  const toasts = createToasts();
  const playback = createPlayback(rpc);
  const sync = createProjectSync({ api, rpc, onError: toasts.showFailure });
  const { calls, busy } = createCalls(toasts, sync.reload);
  // Spread whole: the types let a role see only `Core` and the UI only `CoreReads`.
  const core: Core = { api, dialogs, shell, ...sync, ...calls, ...playback };
  const reads: CoreReads = { ...sync, ...playback, ...toasts, busy };
  /** The first read: the catalog and the project; the views show after it. */
  const start = sync.start;
  return { core, reads, start };
}
