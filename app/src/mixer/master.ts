// The master as the mixer page watches it: the latest window of its sound for the visualizer, the live master meter
// and the mono check. The window and the meter have no on/off: reading them keeps the backend capturing
// (`CAPTURE_LEASE_SECONDS`).
import type { MasterReading, MasterWindow } from "../api/types";
import type { Core } from "../core/core";

export interface Master {
  /** The latest master window (the first read starts the capture); on failure — a toast and undefined (the caller stops
   *  polling then, to avoid a flood of errors). */
  masterWindow(): Promise<MasterWindow | undefined>;
  /** The meter's readings (the first one starts a fresh measurement); on failure — a toast and undefined (the caller
   *  stops polling then). */
  masterMeter(): Promise<MasterReading | undefined>;
  /** Starts the integrated loudness, the loudness range and the held true peak over. */
  resetMasterMeter(): Promise<void>;
  /** The mono check: the speakers get the master summed to mono. Not saved, not exported; the tick shows its state. */
  setMono(on: boolean): Promise<void>;
}

export function createMaster(core: Core): Master {
  const { api } = core;
  return {
    masterWindow: () => core.calls.run(api.audio.masterWindow()),
    masterMeter: () => core.calls.run(api.audio.masterMeter()),
    resetMasterMeter: () => core.calls.exec(api.audio.resetMasterMeter()),
    setMono: (on) => core.calls.exec(api.audio.setMono(on)),
  };
}
