// The instruments role: a track's MIDI effects before its instrument (`noteEffects.ts`); the instrument window itself is a
// component of this folder.
import { defineRole, useRole } from "../core/roles";
import { createNoteEffects, type NoteEffects } from "./noteEffects";

export interface InstrumentsRole {
  /** Track MIDI effects (arpeggiator, chord) before the instrument. */
  readonly noteEffects: NoteEffects;
}

export const instrumentsRole = defineRole("instruments", ({ core }): InstrumentsRole => ({ noteEffects: createNoteEffects(core) }));

export const useInstruments = () => useRole(instrumentsRole);
