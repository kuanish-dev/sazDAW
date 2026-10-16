// Song tempo map actions: tempo and time signature at a point (transport bar, project settings) and time signature
// changes on the ruler. Edits are computed by the pure
// `domain/tempoMap.ts`; only the intent arrives here. Each edit is one backend undo step; the value shows at once (`patchProject`).
import type { SignatureChange, TempoChange, TimeSignature } from "../api/types";
import { withSignatureAt, withSignatureFrom, withTempoAt, withoutSignature } from "../domain/tempoMap";
import type { Backend, Calls, ProjectCopy } from "./context";

export interface TempoActions {
  /** Tempo of the point in effect at `tick` (the transport BPM field uses the playhead, project settings use 0). */
  setTempoAt(tick: number, bpm: number): Promise<void>;
  /** The time signature in effect at `tick` becomes `signature`. */
  setSignatureAt(tick: number, signature: TimeSignature): Promise<void>;
  /** Time signature from the bar containing `tick`. */
  setSignatureFrom(tick: number, signature: TimeSignature): Promise<void>;
  removeSignature(index: number): Promise<void>;
}

export function createTempoActions(ctx: Backend & Calls & ProjectCopy): TempoActions {
  const { api } = ctx;
  const map = () => ctx.project.tempoMap;
  const tempos = (changes: TempoChange[]) => {
    ctx.patchProject((p) => { p.tempoMap.tempos = changes; });
    return ctx.exec(api.tempo.setChanges(changes));
  };
  const signatures = (changes: SignatureChange[]) => {
    ctx.patchProject((p) => { p.tempoMap.signatures = changes; });
    return ctx.exec(api.tempo.setSignatureChanges(changes));
  };
  return {
    setTempoAt: (tick, bpm) => tempos(withTempoAt(map(), tick, bpm)),
    setSignatureAt: (tick, signature) => signatures(withSignatureAt(map(), tick, signature)),
    setSignatureFrom: (tick, signature) => signatures(withSignatureFrom(map(), tick, signature)),
    removeSignature: (index) => signatures(withoutSignature(map(), index)),
  };
}
