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
