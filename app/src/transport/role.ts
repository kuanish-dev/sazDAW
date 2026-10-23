import { defineRole, useRole } from "../core/roles";
import { createTempo, type Tempo } from "./tempo";
import { createTransport, type Transport } from "./transport";

export type TransportRole = Transport & {
  /** Song tempo and meter: tempo points, meter changes. */
  readonly tempo: Tempo;
};

export const transportRole = defineRole("transport", ({ core }): TransportRole => ({
  ...createTransport(core),
  tempo: createTempo(core),
}));

export const useTransport = () => useRole(transportRole);
