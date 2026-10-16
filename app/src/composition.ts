// Composition root: the only place that picks the implementations of the backend link, system dialogs and the window shell.
import type { SystemDialogs } from "./api/dialogs";
import type { RpcClient } from "./api/rpc";
import type { AppShell } from "./api/shell";
import { TauriDialogs } from "./api/tauriDialogs";
import { TauriRpc } from "./api/tauriRpc";
import { TauriShell } from "./api/tauriShell";
import { MockDialogs } from "./mock/MockDialogs";
import { MockRpc } from "./mock/MockRpc";
import { MockShell } from "./mock/MockShell";

const insideTauri = () => "__TAURI_INTERNALS__" in window;

/** Tauri inside the app, the mock in a browser (`?welcome` starts without a project). */
export function createRpc(): RpcClient {
  return insideTauri() ? new TauriRpc() : new MockRpc({ withProject: !location.search.includes("welcome") });
}

export function createDialogs(): SystemDialogs {
  return insideTauri() ? new TauriDialogs() : new MockDialogs();
}

export function createShell(): AppShell {
  return insideTauri() ? new TauriShell() : new MockShell();
}
