// The transport role as the session holds it: playback (`transport.ts`) and the song's tempo and meter (`tempo.ts`).
import type { Core } from "../core/core";
import { createTempoActions, type TempoActions } from "./tempo";
import { createTransportActions, type TransportActions } from "./transport";

export type TransportRole = TransportActions & {
  /** Song tempo and meter: tempo points, meter changes. */
  readonly tempo: TempoActions;
};

export function createTransportRole(core: Core): TransportRole {
  return { ...createTransportActions(core), tempo: createTempoActions(core) };
}
