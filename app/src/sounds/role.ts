// The sounds role: what lies in the project folder and the linked packs (`library.ts`) and a sound from there put to
// work — on the timeline, a track or in the editor (`soundPlacement.ts`).
import { defineRole, useRole } from "../core/roles";
import { samplesRole } from "../samples/role";
import { shellRole } from "../shell/role";
import { createSoundLibrary, type SoundLibrary } from "./library";
import { createSoundPlacement, type SoundPlacement } from "./soundPlacement";

export interface SoundsRole {
  readonly browser: SoundLibrary;
  readonly placement: SoundPlacement;
}

export const soundsRole = defineRole("sounds", ({ core, use }): SoundsRole => ({
  browser: createSoundLibrary(core),
  placement: createSoundPlacement(core, use(shellRole), use(samplesRole)),
}));

export const useSounds = () => useRole(soundsRole);
