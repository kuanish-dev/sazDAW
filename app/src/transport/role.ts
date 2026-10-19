// The transport role: playback (`transport.ts`) and the song's tempo and meter (`tempo.ts`).
import { defineRole, useRole } from "../core/roles";
import { createTempoActions, type TempoActions } from "./tempo";
import { createTransportActions, type TransportActions } from "./transport";

export type TransportRole = TransportActions & {
  /** Song tempo and meter: tempo points, meter changes. */
  readonly tempo: TempoActions;
};

export const transportRole = defineRole("transport", ({ core }): TransportRole => ({
  ...createTransportActions(core),
  tempo: createTempoActions(core),
}));

export const useTransport = () => useRole(transportRole);
