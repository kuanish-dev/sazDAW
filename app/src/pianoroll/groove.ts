// Groove — an exact copy of the backend rules (`daw-project/src/groove.rs`): a table on the 1/16 grid (start offset in
// ticks and velocity in percent per step) from a template or extracted from a source clip, and applying it to notes.
// The mock answers with it; the interface takes the list of preset templates from here.
import { STEP_TICKS, type GrooveTemplate, type Note } from "../api/types";
import { t } from "../i18n";
import { fit, roundAway } from "./noteTransforms";

/** Largest laid-back / pushed shift (`MAX_GROOVE_SHIFT_TICKS`): half a 1/16. */
export const MAX_GROOVE_SHIFT_TICKS = STEP_TICKS / 2;

interface GrooveStep {
  offset: number;
  velocity: number;
}

/** The 1/16 number `i` from the content start takes step `i % steps.length`. */
export type GrooveTable = GrooveStep[];

/** Preset templates in display order; `label` is a function so it follows the interface language. */
export const GROOVE_PRESETS: readonly { label: () => string; template: GrooveTemplate }[] = [
  ...[54, 58, 62, 66].map((percent) => ({ label: () => t().noteTools.grooves.swingEighth(percent), template: { type: "swing", grid: "eighth", percent } as const })),
  ...[54, 58, 62, 66, 71].map((percent) => ({ label: () => t().noteTools.grooves.swingSixteenth(percent), template: { type: "swing", grid: "sixteenth", percent } as const })),
  ...[10, 20, 40].map((ticks) => ({ label: () => t().noteTools.grooves.laidBack(ticks), template: { type: "laidBack", ticks } as const })),
  ...[10, 20, 40].map((ticks) => ({ label: () => t().noteTools.grooves.pushed(ticks), template: { type: "pushed", ticks } as const })),
  ...[115, 130].map((percent) => ({ label: () => t().noteTools.grooves.accent(percent), template: { type: "accent", percent } as const })),
];

/** How much later the second note of a `pair`-tick pair is at swing `percent`, to the tick. */
const swingShift = (pair: number, percent: number) => Math.floor((pair * (percent - 50) + 50) / 100);

export function templateTable(template: GrooveTemplate): GrooveTable {
  const timing = (offsets: number[]) => offsets.map((offset) => ({ offset, velocity: 100 }));
  switch (template.type) {
    case "swing": {
      if (template.grid === "eighth") {
        const eighth = swingShift(960, template.percent);
        const sixteenth = swingShift(480, template.percent);
        return timing([0, sixteenth, eighth, sixteenth]);
      }
      const shift = swingShift(480, template.percent);
      return timing([0, shift, 0, shift]);
    }
    case "laidBack":
      return timing([0, template.ticks, template.ticks, template.ticks]);
    case "pushed":
      return timing([0, -template.ticks, -template.ticks, -template.ticks]);
    case "accent":
      return [{ offset: 0, velocity: template.percent }, ...timing([0, 0, 0])];
  }
}

const stepIndex = (tick: number) => Math.floor((tick + STEP_TICKS / 2) / STEP_TICKS);

/** Groove of a source clip over bar `bar`: mean deviation from the 1/16 and mean step velocity relative to the clip's mean velocity. */
export function extractGroove(notes: Note[], bar: number): GrooveTable {
  if (notes.length === 0) throw t().noteTools.errorEmptyGrooveSource;
  const len = Math.max(Math.floor(bar / STEP_TICKS), 1);
  const sums = Array.from({ length: len }, () => ({ offsets: 0, velocities: 0, count: 0 }));
  for (const note of notes) {
    const index = stepIndex(note.startTick);
    const sum = sums[index % len]!;
    sum.offsets += note.startTick - index * STEP_TICKS;
    sum.velocities += note.velocity;
    sum.count += 1;
  }
  const average = notes.reduce((total, n) => total + n.velocity, 0) / notes.length;
  return sums.map(({ offsets, velocities, count }) =>
    count === 0
      ? { offset: 0, velocity: 100 }
      : { offset: roundAway(offsets / count), velocity: roundAway((100 * (velocities / count)) / average) },
  );
}

export function applyGroove(group: Note[], table: GrooveTable, amount: number, limit: number): Note[] {
  return group.map((note) => {
    const step = table[stepIndex(note.startTick) % table.length]!;
    const shift = roundAway(step.offset * amount);
    const scale = 100 + (step.velocity - 100) * amount;
    const velocity = Math.min(Math.max(roundAway((note.velocity * scale) / 100), 1), 127);
    return fit({ ...note, velocity }, note.startTick + shift, note.lengthTicks, limit);
  });
}
