// The shared core of the interface: what every role asks for its work, as named parts — the backend (`api`, system
// dialogs, the window shell), the project copy (`projectSync.ts`), how a call runs (`calls.ts`), the notifications
// (`notifications.ts`) and the playback tick (`playback.ts`). A role holds the core and asks the part it needs
// (`core.calls.run(…)`, `core.copy.patchProject(…)`), the way the entities on the backend hold the project core. The UI
// never gets the core, only what it may read of its parts (`roles.ts`: `useProjectCopy`, `usePlayback`…).
import { createApi, type DawApi } from "../api/api";
import type { SystemDialogs } from "../api/dialogs";
import type { RpcClient } from "../api/rpc";
import type { AppShell } from "../api/shell";
import { type Calls, createCalls } from "./calls";
import { createNotifications, type Notifications } from "./notifications";
import { createPlayback, type PlaybackReads } from "./playback";
import { createProjectSync, type ProjectCopy } from "./projectSync";

export interface Core {
  /** The backend: typed wrappers over `RpcClient`. */
  readonly api: DawApi;
  readonly dialogs: SystemDialogs;
  /** The window shell: zoom and the app version. */
  readonly appShell: AppShell;
  /** The project as the interface has it. */
  readonly copy: ProjectCopy;
  /** How a call runs and what a failure does. */
  readonly calls: Calls;
  /** The notifications in the window corner. */
  readonly notifications: Notifications;
  /** The playback tick and the positions for drawing. */
  readonly playback: PlaybackReads;
}

export function createCore(rpc: RpcClient, dialogs: SystemDialogs, appShell: AppShell) {
  const api = createApi(rpc);
  const notifications = createNotifications();
  const copy = createProjectSync({ api, rpc, onError: notifications.showFailure });
  const core: Core = {
    api,
    dialogs,
    appShell,
    copy,
    calls: createCalls(notifications, copy.reload),
    notifications,
    playback: createPlayback(rpc),
  };
  /** The first read: the catalog and the project; the views show after it. */
  return { core, start: copy.start };
}
