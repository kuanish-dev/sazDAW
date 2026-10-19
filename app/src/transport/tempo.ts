// Song tempo map actions: tempo and time signature at a point (transport bar, project settings) and time signature
// changes on the ruler. Edits are computed by the pure
// `common/tempoMap.ts`; only the intent arrives here. Each edit is one backend undo step; the value shows at once (`patchProject`).
import type { SignatureChange, TempoChange, TimeSignature } from "../api/types";
import { withSignatureAt, withSignatureFrom, withTempoAt, withoutSignature } from "../common/tempoMap";
import type { Core } from "../core/core";

export interface Tempo {
  /** Tempo of the point in effect at `tick` (the transport BPM field uses the playhead, project settings use 0). */
  setTempoAt(tick: number, bpm: number): Promise<void>;
  /** The time signature in effect at `tick` becomes `signature`. */
  setSignatureAt(tick: number, signature: TimeSignature): Promise<void>;
  /** Time signature from the bar containing `tick`. */
  setSignatureFrom(tick: number, signature: TimeSignature): Promise<void>;
  removeSignature(index: number): Promise<void>;
}

export function createTempo(core: Core): Tempo {
  const { api } = core;
  const map = () => core.copy.project.tempoMap;
  const tempos = (changes: TempoChange[]) => {
    core.copy.patchProject((p) => { p.tempoMap.tempos = changes; });
    return core.calls.exec(api.tempo.setChanges(changes));
  };
  const signatures = (changes: SignatureChange[]) => {
    core.copy.patchProject((p) => { p.tempoMap.signatures = changes; });
    return core.calls.exec(api.tempo.setSignatureChanges(changes));
  };
  return {
    setTempoAt: (tick, bpm) => tempos(withTempoAt(map(), tick, bpm)),
    setSignatureAt: (tick, signature) => signatures(withSignatureAt(map(), tick, signature)),
    setSignatureFrom: (tick, signature) => signatures(withSignatureFrom(map(), tick, signature)),
    removeSignature: (index) => signatures(withoutSignature(map(), index)),
  };
}
