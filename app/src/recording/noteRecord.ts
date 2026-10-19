// Note recording (a MIDI or the computer keyboard into note clips while the transport plays): arming and the grid the
// recorded notes snap to. The transport's shared "● Record" button is `AudioRecordActions.pressRecord`, which asks this.
import type { Core } from "../core/core";

export interface NoteRecordActions {
  setArmed(armed: boolean): Promise<void>;
  /** Grid the recorded notes snap to, in ticks (0 — off). */
  setQuantize(ticks: number): Promise<void>;
}

export function createNoteRecordActions(core: Core): NoteRecordActions {
  const { api } = core;
  return {
    setArmed: (armed) => core.exec(api.record.setArmed(armed)),
    setQuantize: (ticks) => core.exec(api.record.setQuantize(ticks)),
  };
}
