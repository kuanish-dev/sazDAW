// Live MIDI input: available keyboards, connecting one to a track, switching the track and disconnecting.
// The backend doesn't report what is connected, so this run's connection is remembered here (the MIDI section shows it).
import { createSignal } from "solid-js";
import type { Id } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";

export interface MidiConnection {
  device: string;
  track: Id;
}

export interface MidiInput {
  /** Input device names; failure — a toast and an empty list. */
  devices(): Promise<string[]>;
  /** What is connected in this run; `null` — nothing. */
  connection(): MidiConnection | null;
  connect(device: string, track: Id): Promise<void>;
  /** The connected device now plays on `track`. */
  setTarget(track: Id): Promise<void>;
  disconnect(): Promise<void>;
}

export function createMidiInput(core: Core): MidiInput {
  const { api } = core;
  const [connection, setConnection] = createSignal<MidiConnection | null>(null);
  return {
    devices: async () => (await core.calls.run(api.midi.devices())) ?? [],
    connection,
    async connect(device, track) {
      if (!(await core.calls.attempt(api.midi.connect(device, track)))) return;
      setConnection({ device, track });
      core.notifications.toast("info", t().settings.midi.connectedToast(device));
    },
    async setTarget(track) {
      const current = connection();
      if (current && (await core.calls.attempt(api.midi.setTarget(track)))) setConnection({ ...current, track });
    },
    async disconnect() {
      if (await core.calls.attempt(api.midi.disconnect())) setConnection(null);
    },
  };
}
