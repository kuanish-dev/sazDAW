// Which sounds the pattern drums play: the choice per drum, remembered between drum part builds in this run.
import { createSignal } from "solid-js";
import type { LaneChoice } from "../domain/drumPart";

export interface DrumKit {
  /** Drum lane → chosen sound. */
  choices(): Record<string, LaneChoice>;
  pick(lane: string, choice: LaneChoice): void;
}

export function createDrumKit(): DrumKit {
  const [choices, setChoices] = createSignal<Record<string, LaneChoice>>({});
  return {
    choices,
    pick: (lane, choice) => setChoices((kit) => ({ ...kit, [lane]: choice })),
  };
}
