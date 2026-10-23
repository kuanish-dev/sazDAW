import type { AudioDevices, AudioSettings, InputChannels, MonitorMode } from "../api/types";

/** Short label on the arm button. */
export const INPUT_SHORT: Readonly<Record<InputChannels, string>> = { stereo: "1–2", left: "1", right: "2" } as const;

/** Channel count of the selected input device (otherwise the system one); 0 — unknown. */
export function selectedInputChannels(devices: AudioDevices, settings: AudioSettings): number {
  const device = devices.inputs.find((d) => (settings.input === null ? d.isDefault : d.name === settings.input));
  return device?.channels ?? 0;
}

/** Input choices for a device with `channels` channels: a mono input has only channel 1 (unknown — show all). */
export function inputChoices(channels: number): InputChannels[] {
  return channels === 1 ? ["left"] : ["stereo", "left", "right"];
}

/** Monitor button click: off → auto → on → off. */
export function nextMonitorMode(mode: MonitorMode): MonitorMode {
  return mode === "off" ? "auto" : mode === "auto" ? "on" : "off";
}

/** Whether the track hears its input now (as `MonitorMode::hears_input` in the backend). */
export function hearsInput(mode: MonitorMode, armed: boolean): boolean {
  return mode === "on" || (mode === "auto" && armed);
}
