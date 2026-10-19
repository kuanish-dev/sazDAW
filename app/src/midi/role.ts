// The MIDI role: live input from keyboards (`midiInput.ts`) and MIDI learn, controller knobs on parameters
// (`midiLearn.ts`).
import { defineRole, useRole } from "../core/roles";
import { createMidiInputActions, type MidiInputActions } from "./midiInput";
import { createMidiLearnActions, type MidiLearnActions } from "./midiLearn";

export interface MidiRole {
  readonly input: MidiInputActions;
  readonly learn: MidiLearnActions;
}

export const midiRole = defineRole("midi", ({ core }): MidiRole => {
  const input = createMidiInputActions(core);
  return { input, learn: createMidiLearnActions(core, input) };
});

export const useMidi = () => useRole(midiRole);
