// The drums role: building a drum part from patterns (`drums.ts`) and which sounds the pattern drums play, remembered
// between builds (`drumKit.ts`).
import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { createDrumKit, type DrumKit } from "./drumKit";
import { createDrumsActions, type DrumsActions } from "./drums";

export type DrumsRole = DrumsActions & {
  readonly kit: DrumKit;
};

export const drumsRole = defineRole("drums", ({ core, use }): DrumsRole => ({ ...createDrumsActions(core, use(shellRole)), kit: createDrumKit() }));

export const useDrums = () => useRole(drumsRole);
