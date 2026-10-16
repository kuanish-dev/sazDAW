// Note recording (a MIDI or the computer keyboard into note clips while the transport plays): arming and the grid the
// recorded notes snap to. The transport's shared "● Record" button is `AudioRecordActions.pressRecord`, which asks this.
import type { Backend, Calls } from "./context";

export interface NoteRecordActions {
  setArmed(armed: boolean): Promise<void>;
  /** Grid the recorded notes snap to, in ticks (0 — off). */
  setQuantize(ticks: number): Promise<void>;
}

export function createNoteRecordActions(ctx: Backend & Calls): NoteRecordActions {
  const { api } = ctx;
  return {
    setArmed: (armed) => ctx.exec(api.record.setArmed(armed)),
    setQuantize: (ticks) => ctx.exec(api.record.setQuantize(ticks)),
  };
}
