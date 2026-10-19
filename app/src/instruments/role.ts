// The instruments role as the session holds it: a track's MIDI effects before its instrument (`noteEffects.ts`); the
// instrument window itself is a component of this folder.
import type { Core } from "../core/core";
import { createNoteEffectsActions, type NoteEffectsActions } from "./noteEffects";

export interface InstrumentsRole {
  /** Track MIDI effects (arpeggiator, chord) before the instrument. */
  readonly noteEffects: NoteEffectsActions;
}

export function createInstrumentsRole(core: Core): InstrumentsRole {
  return { noteEffects: createNoteEffectsActions(core) };
}
