// Starting the window: the shared core over the backend `composition.ts` picked (Tauri or the mock), the registry of
// roles made from it, and the order the first things happen in.
import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { createCore } from "./core/core";
import { createRoles, type Roles } from "./core/roles";
import { filesRole } from "./files/role";
import { recordingRole } from "./recording/role";
import { settingsRole } from "./settings/role";

export function startApp(rpc: RpcClient, dialogs: SystemDialogs, shell: AppShell): Roles {
  const { core, start } = createCore(rpc, dialogs, shell);
  const roles = createRoles(core);
  void startup(roles, start);
  return roles;
}

/**
 * Recording at once (track arming lives in the backend longer than the window), the preferences next (the look, the
 * projects folder, "open last project"), then the project, then what opening it left to say: everything is in place
 * when the views show.
 */
async function startup(roles: Roles, firstRead: () => Promise<void>) {
  roles.use(recordingRole);
  await roles.use(settingsRole).preferences.load();
  await firstRead();
  await roles.use(filesRole).startup.afterFirstRead();
}
