import { defineRole, useRole } from "../core/roles";
import { createLevels, createReductions, type Levels, type Reductions } from "./levels";
import { createMaster, type Master } from "./master";

export interface MixerRole {
  readonly levels: Levels;
  readonly reductions: Reductions;
  readonly master: Master;
}

export const mixerRole = defineRole("mixer", ({ core }): MixerRole => ({
  levels: createLevels(core.playback.tick),
  reductions: createReductions(core.playback.tick),
  master: createMaster(core),
}));

export const useMixer = () => useRole(mixerRole);
