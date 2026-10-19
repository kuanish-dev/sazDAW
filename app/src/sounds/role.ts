// The sounds role: what lies in the project folder and the linked packs (`library.ts`) and a sound from there put to
// work — on the timeline, a track or in the editor (`soundPlacement.ts`).
import { defineRole, useRole } from "../core/roles";
import { samplesRole } from "../samples/role";
import { shellRole } from "../shell/role";
import { createSoundBrowserActions, type SoundBrowserActions } from "./library";
import { createSoundPlacementActions, type SoundPlacementActions } from "./soundPlacement";

export interface SoundsRole {
  readonly browser: SoundBrowserActions;
  readonly placement: SoundPlacementActions;
}

export const soundsRole = defineRole("sounds", ({ core, use }): SoundsRole => ({
  browser: createSoundBrowserActions(core),
  placement: createSoundPlacementActions(core, use(shellRole), use(samplesRole)),
}));

export const useSounds = () => useRole(soundsRole);
