// Backend communication contract. Implementations: `tauriRpc.ts` (app) and `mock/MockRpc.ts` (browser).
import type { Tick } from "./types";

export interface RpcClient {
  call<T = void>(method: string, params?: unknown): Promise<T>;
  /** High-frequency data (position, meters); returns an unsubscribe function. */
  onTick(listener: (tick: Tick) => void): () => void;
  /** The project changed (new revision); returns an unsubscribe function. */
  onProject(listener: (revision: number) => void): () => void;
}
