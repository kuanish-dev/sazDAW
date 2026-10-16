// Track MIDI effects in the mock: the chain in the model with the backend's checks and error codes (the mock processes no sound).
import type { Id, NoteEffectSpec, Params, Project } from "../../api/types";
import { t } from "../../i18n";
import { fitParam, paramsFor } from "../catalog";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** The backend's `MAX_NOTE_EFFECTS`. */
const MAX_NOTE_EFFECTS = 8;

export function noteEffectsHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  const chainOf = (p: Project, track: Id): NoteEffectSpec[] => {
    const found = w.track(p, track);
    if (!found.instrument) throw fail("NT-113", { name: found.name });
    found.noteEffects ??= [];
    return found.noteEffects;
  };
  const effectOf = (p: Project, track: Id, effect: Id) => w.must(chainOf(p, track).find((e) => e.id === effect), fail("NT-011", { id: effect }));
  const kindOf = (kind: string) => {
    const info = w.catalog.noteEffects.find((k) => k.kind === kind);
    if (!info) throw fail("NT-002", { kind });
    return info;
  };

  h["noteEffects.add"] = ({ track, kind, params }: { track: Id; kind: string; params?: Params }) => {
    const values = paramsFor(kindOf(kind), params ?? {}, "NT-121");
    // A failure inside `edit` changes nothing: the edit runs on a project copy.
    return w.edit(t().mockEdits.history.addMidiEffect, (p) => {
      const chain = chainOf(p, track);
      if (chain.length >= MAX_NOTE_EFFECTS) throw fail("NT-230", { max: MAX_NOTE_EFFECTS });
      const id = w.id(p);
      chain.push({ id, kind, params: values, bypass: false });
      return id;
    });
  };
  h["noteEffects.remove"] = ({ track, effect }: { track: Id; effect: Id }) => {
    return w.edit(t().mockEdits.history.removeMidiEffect, (p) => {
      effectOf(p, track, effect);
      w.track(p, track).noteEffects = chainOf(p, track).filter((e) => e.id !== effect);
    });
  };
  h["noteEffects.move"] = ({ track, effect, index }: { track: Id; effect: Id; index: number }) => {
    return w.edit(t().mockEdits.history.moveMidiEffect, (p) => {
      const chain = chainOf(p, track);
      const spec = effectOf(p, track, effect);
      chain.splice(chain.indexOf(spec), 1);
      chain.splice(Math.min(index, chain.length), 0, spec);
    });
  };
  h["noteEffects.setParam"] = ({ track, effect, name, value }: { track: Id; effect: Id; name: string; value: number }) => {
    if (!Number.isFinite(value)) throw fail("NT-112", { value });
    return w.edit(t().mockEdits.history.midiEffectParam, (p) => {
      const spec = effectOf(p, track, effect);
      const info = kindOf(spec.kind).params.find((x) => x.name === name);
      if (!info) throw fail("NT-121", { kind: spec.kind, param: name });
      spec.params[name] = fitParam(info, value);
    });
  };
  h["noteEffects.setBypass"] = ({ track, effect, bypass }: { track: Id; effect: Id; bypass: boolean }) => {
    return w.edit(t().mockEdits.history.midiEffectBypass, (p) => {
      effectOf(p, track, effect).bypass = bypass;
    });
  };
  return h;
}
