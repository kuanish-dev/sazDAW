import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { RolesProvider } from "./core/roles";
import { AppWindow } from "./shell/AppWindow";
import { startApp } from "./start";

export function App(props: { rpc: RpcClient; dialogs: SystemDialogs; shell: AppShell }) {
  const roles = startApp(props.rpc, props.dialogs, props.shell);
  return (
    <RolesProvider roles={roles}>
      <AppWindow />
    </RolesProvider>
  );
}
