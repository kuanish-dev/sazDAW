// The mixer role as the session holds it: the meters of every node and the effects' gain reduction (`levels.ts`), and
// the master — its window for the visualizer, the live meter, the mono check (`master.ts`).
import type { Core } from "../core/core";
import { createLevels, createReductions, type Levels, type Reductions } from "./levels";
import { createMasterActions, type MasterActions } from "./master";

export interface MixerRole {
  readonly levels: Levels;
  readonly reductions: Reductions;
  readonly master: MasterActions;
}

export function createMixerRole(core: Core): MixerRole {
  return { levels: createLevels(core.tick), reductions: createReductions(core.tick), master: createMasterActions(core) };
}
