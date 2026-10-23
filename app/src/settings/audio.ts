import type { AudioDevices, AudioSettings, AudioStatus, InputLevel } from "../api/types";
import { MAX_RECORD_LATENCY_MS, parseBufferFrames, parseRecordLatency } from "./audioSettings";
import { t } from "../i18n";
import type { Core } from "../core/core";

/** Settings form fields as the user typed them: an empty string means "default". */
export interface AudioSettingsForm {
  output: string;
  input: string;
  buffer: string;
  recordLatency: string;
}

export interface AudioSetup {
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
}

export function createAudioSetup(core: Core): AudioSetup {
  const { api } = core;
  return {
    devices: () => core.calls.run(api.audio.devices()),
    settings: () => core.calls.run(api.audio.settings()),
    status: () => core.calls.run(api.audio.status()),
    async apply(form) {
      const recordLatencyMs = parseRecordLatency(form.recordLatency);
      if (recordLatencyMs === undefined) {
        core.notifications.toast("error", t().settings.audio.errorRecordLatency(MAX_RECORD_LATENCY_MS));
        return null;
      }
      const settings: AudioSettings = {
        output: form.output || null,
        input: form.input || null,
        bufferFrames: parseBufferFrames(form.buffer),
        recordLatencyMs,
      };
      const status = await core.calls.run(api.audio.apply(settings));
      if (!status) return (await core.calls.run(api.audio.status())) ?? null;
      core.notifications.toast("info", t().settings.audio.switched(status.outputName ?? t().settings.audio.defaultDeviceName));
      return status;
    },
    watchInput: (on) => core.calls.exec(api.audio.watchInput(on)),
    inputLevel: () => core.calls.run(api.audio.inputLevel()),
  };
}
