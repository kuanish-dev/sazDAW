// The instruments role: a track's MIDI effects before its instrument (`noteEffects.ts`); the instrument window itself is a
// component of this folder.
import { defineRole, useRole } from "../core/roles";
import { createNoteEffectsActions, type NoteEffectsActions } from "./noteEffects";

export interface InstrumentsRole {
  /** Track MIDI effects (arpeggiator, chord) before the instrument. */
  readonly noteEffects: NoteEffectsActions;
}

export const instrumentsRole = defineRole("instruments", ({ core }): InstrumentsRole => ({ noteEffects: createNoteEffectsActions(core) }));

export const useInstruments = () => useRole(instrumentsRole);
