// The MIDI role as the session holds it: live input from keyboards (`midiInput.ts`) and MIDI learn, controller knobs on
// parameters (`midiLearn.ts`).
import type { Core } from "../core/core";
import { createMidiInputActions, type MidiInputActions } from "./midiInput";
import { createMidiLearnActions, type MidiLearnActions } from "./midiLearn";

export interface MidiRole {
  readonly input: MidiInputActions;
  readonly learn: MidiLearnActions;
}

export function createMidiRole(core: Core): MidiRole {
  const input = createMidiInputActions(core);
  return { input, learn: createMidiLearnActions(core, input) };
}
