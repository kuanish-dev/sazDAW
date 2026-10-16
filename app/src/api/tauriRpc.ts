// `RpcClient` implementation for the app: calls go through the Tauri `rpc` command; subscriptions use the `tick` and `project` events.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RpcClient } from "./rpc";
import type { Tick } from "./types";

/** Subscribes to a Tauri event, returning a synchronous unsubscribe function. */
function subscribe<T>(event: string, listener: (payload: T) => void): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;
  listen<T>(event, (e) => listener(e.payload))
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    // Without the subscription the UI would silently stop updating, so the failure is logged (console.error → errorReporter).
    .catch((error) => console.error(`listen(${event})`, error));
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

export class TauriRpc implements RpcClient {
  call<T = void>(method: string, params?: unknown): Promise<T> {
    return invoke<T>("rpc", { method, params: params ?? null });
  }

  onTick(listener: (tick: Tick) => void) {
    return subscribe<Tick>("tick", listener);
  }

  onProject(listener: (revision: number) => void) {
    return subscribe<number>("project", listener);
  }
}
