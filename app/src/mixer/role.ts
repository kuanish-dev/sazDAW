// The mixer role: the meters of every node and the effects' gain reduction (`levels.ts`), and the master — its window for
// the visualizer, the live meter, the mono check (`master.ts`).
import { defineRole, useRole } from "../core/roles";
import { createLevels, createReductions, type Levels, type Reductions } from "./levels";
import { createMasterActions, type MasterActions } from "./master";

export interface MixerRole {
  readonly levels: Levels;
  readonly reductions: Reductions;
  readonly master: MasterActions;
}

export const mixerRole = defineRole("mixer", ({ core }): MixerRole => ({
  levels: createLevels(core.tick),
  reductions: createReductions(core.tick),
  master: createMasterActions(core),
}));

export const useMixer = () => useRole(mixerRole);
