import { defineRole, useRole } from "../core/roles";
import { createNoteEffects, type NoteEffects } from "./noteEffects";

export interface InstrumentsRole {
  /** Track MIDI effects (arpeggiator, chord) before the instrument. */
  readonly noteEffects: NoteEffects;
}

export const instrumentsRole = defineRole("instruments", ({ core }): InstrumentsRole => ({ noteEffects: createNoteEffects(core) }));

export const useInstruments = () => useRole(instrumentsRole);
