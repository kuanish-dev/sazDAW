// Backend communication contract. Implementations: `tauriRpc.ts` (app) and `mock/MockRpc.ts` (browser).
import type { ProjectUpdate, Tick } from "./types";

export interface RpcClient {
  call<T = void>(method: string, params?: unknown): Promise<T>;
  /** The reply as raw bytes, for the methods read many times a second (the live sound windows; the format and its
   *  decoding are in `wire.ts`). Any other method is refused with `RP-502`. */
  callBinary(method: string, params?: unknown): Promise<ArrayBuffer>;
  /** High-frequency data (position, meters); returns an unsubscribe function. */
  onTick(listener: (tick: Tick) => void): () => void;
  /**
   * The project as it changes (the project feed): right after subscribing, the listener gets the whole project
   * (`whole`), then each update brings what changed since the one before. An update that comes before the first whole
   * one goes on top of a copy the listener does not have yet: it skips it. Returns an unsubscribe function.
   */
  onProject(listener: (update: ProjectUpdate) => void): () => void;
  /** Asks for the whole project with the next update (after a refused call: the copy may show a value the backend did
   *  not take). */
  askWholeProject(): Promise<void>;
}
