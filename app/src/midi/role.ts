// The MIDI role: live input from keyboards (`midiInput.ts`) and MIDI learn, controller knobs on parameters
// (`midiLearn.ts`).
import { defineRole, useRole } from "../core/roles";
import { createMidiInput, type MidiInput } from "./midiInput";
import { createMidiLearn, type MidiLearn } from "./midiLearn";

export interface MidiRole {
  readonly input: MidiInput;
  readonly learn: MidiLearn;
}

export const midiRole = defineRole("midi", ({ core }): MidiRole => {
  const input = createMidiInput(core);
  return { input, learn: createMidiLearn(core, input) };
});

export const useMidi = () => useRole(midiRole);
