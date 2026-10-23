import type { NoteEffectSpec } from "../api/types";

/** Labels of built-in kinds; an unknown kind uses its first three letters. */
const BADGES: Readonly<Record<string, string>> = { arp: "ARP", chord: "CH", repeat: "RPT" } as const;

export const noteEffectBadge = (kind: string): string => BADGES[kind] ?? kind.slice(0, 3).toUpperCase();

/** Label of non-bypassed effects in chain order; `undefined` if all are bypassed or there is no chain. */
export function chainBadge(effects: NoteEffectSpec[] | undefined): string | undefined {
  const active = (effects ?? []).filter((e) => !e.bypass).map((e) => noteEffectBadge(e.kind));
  return active.length > 0 ? active.join("·") : undefined;
}
