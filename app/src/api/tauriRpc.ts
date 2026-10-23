import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RpcClient } from "./rpc";
import type { ProjectUpdate, Tick } from "./types";

/** Subscribes to a Tauri event, returning a synchronous unsubscribe function; `listening` runs once the subscription
 *  holds (an event sent before it is not heard). */
function subscribe<T>(event: string, listener: (payload: T) => void, listening?: () => void): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;
  listen<T>(event, (e) => listener(e.payload))
    .then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unlisten = fn;
      listening?.();
    })
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

  callBinary(method: string, params?: unknown): Promise<ArrayBuffer> {
    return invoke<ArrayBuffer>("rpc_binary", { method, params: params ?? null });
  }

  onTick(listener: (tick: Tick) => void) {
    return subscribe<Tick>("tick", listener);
  }

  onProject(listener: (update: ProjectUpdate) => void) {
    return subscribe<ProjectUpdate>("project", listener, () => {
      this.askWholeProject().catch((error) => console.error("project_whole", error));
    });
  }

  askWholeProject(): Promise<void> {
    return invoke("project_whole");
  }
}
