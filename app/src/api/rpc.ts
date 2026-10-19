// Backend communication contract. Implementations: `tauriRpc.ts` (app) and `mock/MockRpc.ts` (browser).
import type { Tick } from "./types";

export interface RpcClient {
  call<T = void>(method: string, params?: unknown): Promise<T>;
  /** The reply as raw bytes, for the methods read many times a second (the live sound windows; the format and its
   *  decoding are in `wire.ts`). Any other method is refused with `RP-502`. */
  callBinary(method: string, params?: unknown): Promise<ArrayBuffer>;
  /** High-frequency data (position, meters); returns an unsubscribe function. */
  onTick(listener: (tick: Tick) => void): () => void;
  /** The project changed (new revision); returns an unsubscribe function. */
  onProject(listener: (revision: number) => void): () => void;
}
