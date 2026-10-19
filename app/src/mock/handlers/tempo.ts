// The song's tempo map (`tempo.*`): edits are undo steps; an invalid map is refused with the backend's codes
// (as `TempoMap::new` in `daw-contracts/src/tempo_map.rs`).
import type { SignatureChange, TempoChange, TempoMap } from "../../api/types";
import { MAX_SIGNATURE_CHANGES, MAX_TEMPO_CHANGES, MAX_TICK } from "../../common/tempoMap";
import { barTicks, MAX_NUMERATOR, MAX_TEMPO, MIN_TEMPO } from "../../common/time";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const DENOMINATORS: readonly number[] = [1, 2, 4, 8, 16, 32] as const;

function checkBpm(bpm: number) {
  if (!(bpm >= MIN_TEMPO && bpm <= MAX_TEMPO)) throw fail("TM-101", { bpm, min: MIN_TEMPO, max: MAX_TEMPO });
}

function checkSignature({ numerator, denominator }: { numerator: number; denominator: number }) {
  if (!DENOMINATORS.includes(denominator) || numerator < 1 || numerator > MAX_NUMERATOR) throw fail("TM-102", { numerator, denominator });
}

/** Point ticks of list `list`: some, not too many, the first at 0, strictly increasing, within the song. */
function checkTicks(list: string, ticks: number[], max: number) {
  if (ticks.length === 0) throw fail("TM-103", { list });
  if (ticks.length > max) throw fail("TM-201", { list, max });
  ticks.forEach((tick, i) => {
    if (i === 0 && tick !== 0) throw fail("TM-104", { list, tick });
    if (i > 0 && tick <= ticks[i - 1]!) throw fail("TM-105", { list, before: ticks[i - 1]!, tick });
    if (tick > MAX_TICK) throw fail("TM-202", { tick, max: MAX_TICK });
  });
}

function checkMap(map: TempoMap) {
  checkTicks("tempos", map.tempos.map((c) => c.tick), MAX_TEMPO_CHANGES);
  checkTicks("signatures", map.signatures.map((c) => c.tick), MAX_SIGNATURE_CHANGES);
  map.signatures.forEach((change, i) => {
    const before = map.signatures[i - 1];
    if (!before) return;
    const bar = barTicks(before.signature);
    if ((change.tick - before.tick) % bar !== 0) {
      const { numerator, denominator } = before.signature;
      throw fail("TM-106", { tick: change.tick, numerator, denominator, bar, from: before.tick });
    }
  });
}

/** Replaces the whole map in one edit; an invalid map is an error and the project stays unchanged. */
function replace(w: MockWorld, label: string, next: TempoMap) {
  for (const change of next.tempos) checkBpm(change.bpm);
  for (const change of next.signatures) checkSignature(change.signature);
  checkMap(next);
  // As in the backend: the first point never ramps.
  const tempos = next.tempos.map((c, i) => ({ ...c, ramp: i > 0 && Boolean(c.ramp) }));
  w.edit(label, (p) => { p.tempoMap = { tempos, signatures: next.signatures }; });
}

export function tempoHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const map = () => w.project.tempoMap;
  h["tempo.get"] = () => w.clone(map());
  h["tempo.setChanges"] = ({ changes }: { changes: TempoChange[] }) => replace(w, t().mockEdits.history.changeTempo, { ...map(), tempos: changes });
  h["tempo.setSignatureChanges"] = ({ changes }: { changes: SignatureChange[] }) =>
    replace(w, t().mockEdits.history.changeSignature, { ...map(), signatures: changes });
  h["tempo.set"] = ({ bpm }) => replace(w, t().mockEdits.history.changeTempo, { ...map(), tempos: map().tempos.map((c, i) => (i === 0 ? { ...c, bpm } : c)) });
  h["tempo.setSignature"] = ({ numerator, denominator }) => {
    const signatures = map().signatures.map((c, i) => (i === 0 ? { ...c, signature: { numerator, denominator } } : c));
    replace(w, t().mockEdits.history.changeSignature, { ...map(), signatures });
  };
  // The browser has no files: the chosen "file" carries a map with two tempos and a signature change, so there is something to see.
  h["midi.importTempo"] = () => replace(w, t().mockEdits.history.changeTempoAndSignature, {
    tempos: [{ tick: 0, bpm: 120, ramp: false }, { tick: 15_360, bpm: 140, ramp: true }],
    signatures: [{ tick: 0, signature: { numerator: 4, denominator: 4 } }, { tick: 30_720, signature: { numerator: 7, denominator: 8 } }],
  });
  return h;
}
