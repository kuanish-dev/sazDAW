// Live MIDI input: available keyboards, connecting one to a track, switching the track and disconnecting.
// The backend doesn't report what is connected, so this run's connection is remembered here (the MIDI section shows it).
import { createSignal } from "solid-js";
import type { Id } from "../api/types";
import { t } from "../i18n";
import type { Backend, Calls } from "./context";

export interface MidiConnection {
  device: string;
  track: Id;
}

export interface MidiInputActions {
  /** Input device names; failure — a toast and an empty list. */
  devices(): Promise<string[]>;
  /** What is connected in this run; `null` — nothing. */
  connection(): MidiConnection | null;
  connect(device: string, track: Id): Promise<void>;
  /** The connected device now plays on `track`. */
  setTarget(track: Id): Promise<void>;
  disconnect(): Promise<void>;
}

export function createMidiInputActions(ctx: Backend & Calls): MidiInputActions {
  const { api } = ctx;
  const [connection, setConnection] = createSignal<MidiConnection | null>(null);
  return {
    devices: async () => (await ctx.run(api.midi.devices())) ?? [],
    connection,
    async connect(device, track) {
      if (!(await ctx.attempt(api.midi.connect(device, track)))) return;
      setConnection({ device, track });
      ctx.toast("info", t().settings.midi.connectedToast(device));
    },
    async setTarget(track) {
      const current = connection();
      if (current && (await ctx.attempt(api.midi.setTarget(track)))) setConnection({ ...current, track });
    },
    async disconnect() {
      if (await ctx.attempt(api.midi.disconnect())) setConnection(null);
    },
  };
}
