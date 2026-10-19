// Catalog lookups: instrument, effect and parameter descriptions; parameter values with defaults filled in.
import type { Catalog, KindInfo, ParamInfo, Params } from "../api/types";

export function instrumentInfo(catalog: Catalog | null, kind: string | undefined): KindInfo | undefined {
  return kind === undefined ? undefined : catalog?.instruments.find((k) => k.kind === kind);
}

export function effectInfo(catalog: Catalog | null, kind: string | undefined): KindInfo | undefined {
  return kind === undefined ? undefined : catalog?.effects.find((k) => k.kind === kind);
}

export function noteEffectInfo(catalog: Catalog | null, kind: string | undefined): KindInfo | undefined {
  return kind === undefined ? undefined : catalog?.noteEffects.find((k) => k.kind === kind);
}

export function paramInfo(info: KindInfo | undefined, name: string): ParamInfo | undefined {
  return info?.params.find((p) => p.name === name);
}

/** Kinds as choices for a picker: the kind id is the value, its catalog label the text. */
export const kindChoices = (kinds: readonly KindInfo[]): { value: string; label: string }[] =>
  kinds.map((k) => ({ value: k.kind, label: k.label }));

/** Parameter value; if the project does not set it, the catalog default. */
export const paramValue = (params: Params, info: ParamInfo): number => params[info.name] ?? info.default;

/** Values of all parameters of a kind (for the envelope graph, etc.). */
export function paramValues(params: Params, infos: ParamInfo[]): Params {
  return Object.fromEntries(infos.map((info) => [info.name, paramValue(params, info)]));
}

/** The named params with defaults filled in; `null` when the kind lacks one of them (the effect graphs read their
 *  settings this way, so a changed catalog shows no graph rather than a wrong one). */
export function namedParams<N extends string>(infos: readonly ParamInfo[], params: Params, names: readonly N[]): Record<N, number> | null {
  const values: Partial<Record<N, number>> = {};
  for (const name of names) {
    const info = infos.find((p) => p.name === name);
    if (!info) return null;
    values[name] = paramValue(params, info);
  }
  return values as Record<N, number>;
}

/** A param change made by dragging a handle on an effect graph (EQ band, filter cutoff), applied like a knob turn. */
export interface ParamUpdate {
  name: string;
  value: number;
}
