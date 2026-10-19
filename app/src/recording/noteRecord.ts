// Note recording (a MIDI or the computer keyboard into note clips while the transport plays): arming and the grid the
// recorded notes snap to. The transport's shared "● Record" button is `AudioRecord.pressRecord`, which asks this.
import type { Core } from "../core/core";

export interface NoteRecord {
  setArmed(armed: boolean): Promise<void>;
  /** Grid the recorded notes snap to, in ticks (0 — off). */
  setQuantize(ticks: number): Promise<void>;
}

export function createNoteRecord(core: Core): NoteRecord {
  const { api } = core;
  return {
    setArmed: (armed) => core.calls.exec(api.record.setArmed(armed)),
    setQuantize: (ticks) => core.calls.exec(api.record.setQuantize(ticks)),
  };
}
