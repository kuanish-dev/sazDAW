// Audio actions: devices, buffer size, recording latency, input level check, the master window for the visualizer, the
// nodes' windows for their pictures (the live effect visuals), the live master meter and the mono check. The windows and
// the meter have no on/off: reading them keeps the backend capturing (`CAPTURE_LEASE_SECONDS`).
import type { AudioDevices, AudioSettings, AudioStatus, Id, InputLevel, MasterReading, MasterWindow, NodeCursor, NodeWindow } from "../api/types";
import { MAX_RECORD_LATENCY_MS, parseBufferFrames, parseRecordLatency } from "../domain/audioSettings";
import { t } from "../i18n";
import type { Backend, Calls } from "./context";

/** Settings form fields as the user typed them: an empty string means "default". */
export interface AudioSettingsForm {
  output: string;
  input: string;
  buffer: string;
  recordLatency: string;
}

export interface AudioActions {
  devices(): Promise<AudioDevices | undefined>;
  settings(): Promise<AudioSettings | undefined>;
  status(): Promise<AudioStatus | undefined>;
  /**
   * Validates the fields and switches audio; success — an "Audio switched" toast. Invalid latency — a toast and `null` (nothing changed);
   * the device failed to open — a toast and the current audio status (unchanged).
   */
  apply(form: AudioSettingsForm): Promise<AudioStatus | null>;
  /** The input is open only while the level is being checked. */
  watchInput(on: boolean): Promise<void>;
  /** Current input level; on failure — a toast and undefined (the section then stops polling to avoid a flood of errors). */
  inputLevel(): Promise<InputLevel | undefined>;
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
  /** The new frames of `nodes` since `seen` (`state/nodeSounds.ts` reads every node shown in one call); on failure — a
   *  toast and undefined (the caller stops polling then). */
  nodeWindows(nodes: Id[], seen: NodeCursor[]): Promise<NodeWindow[] | undefined>;
}

export function createAudioActions(ctx: Backend & Calls): AudioActions {
  const { api } = ctx;
  return {
    devices: () => ctx.run(api.audio.devices()),
    settings: () => ctx.run(api.audio.settings()),
    status: () => ctx.run(api.audio.status()),
    async apply(form) {
      const recordLatencyMs = parseRecordLatency(form.recordLatency);
      if (recordLatencyMs === undefined) {
        ctx.toast("error", t().settings.audio.errorRecordLatency(MAX_RECORD_LATENCY_MS));
        return null;
      }
      const settings: AudioSettings = {
        output: form.output || null,
        input: form.input || null,
        bufferFrames: parseBufferFrames(form.buffer),
        recordLatencyMs,
      };
      const status = await ctx.run(api.audio.apply(settings));
      if (!status) return (await ctx.run(api.audio.status())) ?? null;
      ctx.toast("info", t().settings.audio.switched(status.outputName ?? t().settings.audio.defaultDeviceName));
      return status;
    },
    watchInput: (on) => ctx.exec(api.audio.watchInput(on)),
    inputLevel: () => ctx.run(api.audio.inputLevel()),
    masterWindow: () => ctx.run(api.audio.masterWindow()),
    masterMeter: () => ctx.run(api.audio.masterMeter()),
    resetMasterMeter: () => ctx.exec(api.audio.resetMasterMeter()),
    setMono: (on) => ctx.exec(api.audio.setMono(on)),
    nodeWindows: (nodes, seen) => ctx.run(api.audio.nodeWindows(nodes, seen)),
  };
}
