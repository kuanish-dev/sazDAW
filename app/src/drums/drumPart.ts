import type { DrumPartRequest, DrumPattern, LaneAssignment, LaneTarget, PatternRole, PatternSlot, Project } from "../api/types";
import { t } from "../i18n";

/** Pattern roles in display order; their names are `t().drums.roles`. */
export const ROLES: readonly PatternRole[] = ["groove", "fill", "break", "intro", "ending"] as const;

/** Drum name for grid labels and new track names; `lane` is an instrument kind id. */
export const laneLabel = (lane: string): string => t().drums.lanes[lane] ?? lane;

/** How many bars the part takes. A pattern missing from the list counts as zero bars. */
export function totalBars(slots: PatternSlot[], patterns: DrumPattern[]): number {
  return slots.reduce((sum, slot) => sum + (patterns.find((p) => p.id === slot.pattern)?.bars ?? 0) * slot.repeat, 0);
}

/** Genres among a role's patterns, in order of first appearance. */
export function stylesOf(patterns: DrumPattern[], role: PatternRole): string[] {
  return [...new Set(patterns.filter((p) => p.role === role).map((p) => p.style))];
}

/**
 * A drum's sound choice in the list: `auto`, `skip`, `track:<id>` or `sample:<id>`.
 * A string because a `<select>` holds it; `choiceTarget` turns it into the request form.
 */
export type LaneChoice = string;

export const AUTO_CHOICE: LaneChoice = "auto";
export const SKIP_CHOICE: LaneChoice = "skip";
export const trackChoice = (id: number): LaneChoice => `track:${id}`;
export const sampleChoice = (id: number): LaneChoice => `sample:${id}`;

export function choiceTarget(choice: LaneChoice): LaneTarget {
  const [kind, id] = choice.split(":");
  if (kind === "skip") return { type: "skip" };
  if (kind === "track" && id) return { type: "track", id: Number(id) };
  if (kind === "sample" && id) return { type: "sample", id: Number(id) };
  return { type: "auto" };
}

/** Whether what the choice points to still exists (the track or sample may have been deleted). */
export function choiceExists(choice: LaneChoice, project: Pick<Project, "tracks" | "samples">): boolean {
  return choice === SKIP_CHOICE
    || project.tracks.some((track) => track.instrument !== null && trackChoice(track.id) === choice)
    || project.samples.some((s) => sampleChoice(s.id) === choice);
}

/** What plays the drum: the kit's choice; a vanished or missing choice falls back to "auto". */
export function effectiveChoice(kit: Record<string, LaneChoice>, lane: string, project: Pick<Project, "tracks" | "samples">): LaneChoice {
  const choice = kit[lane] ?? AUTO_CHOICE;
  return choice === AUTO_CHOICE || choiceExists(choice, project) ? choice : AUTO_CHOICE;
}

/** The part's drums in order of first appearance. */
export function lanesOf(slots: PatternSlot[], patterns: DrumPattern[]): string[] {
  const lanes: string[] = [];
  for (const slot of slots) {
    for (const { lane } of patterns.find((p) => p.id === slot.pattern)?.lanes ?? []) {
      if (!lanes.includes(lane)) lanes.push(lane);
    }
  }
  return lanes;
}

/** Sound assignments: only the part's drums whose effective choice is not "auto". */
function assignmentsOf(kit: Record<string, LaneChoice>, lanes: string[], project: Pick<Project, "tracks" | "samples">): LaneAssignment[] {
  return lanes.flatMap((lane) => {
    const choice = effectiveChoice(kit, lane, project);
    return choice === AUTO_CHOICE ? [] : [{ lane, target: choiceTarget(choice) }];
  });
}

/** Request to build (or preview) part `slots` with the chosen drum sounds. */
export function drumRequest(
  part: { startBar: number; slots: PatternSlot[]; replace: boolean },
  kit: Record<string, LaneChoice>,
  patterns: DrumPattern[],
  project: Pick<Project, "tracks" | "samples">,
): DrumPartRequest {
  return { ...part, lanes: assignmentsOf(kit, lanesOf(part.slots, patterns), project) };
}

/** The list with item `index` swapped with the one `by` away; unchanged past the edge. */
export function moveItem<T>(list: T[], index: number, by: number): T[] {
  const target = index + by;
  if (index < 0 || index >= list.length || target < 0 || target >= list.length) return list;
  const next = [...list];
  [next[index], next[target]] = [next[target]!, next[index]!];
  return next;
}
