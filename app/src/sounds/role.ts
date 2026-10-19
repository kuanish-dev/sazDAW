// The sounds role as the session holds it: what lies in the project folder and the linked packs (`library.ts`) and a
// sound from there put to work — on the timeline, a track or in the editor (`soundPlacement.ts`).
import type { Core } from "../core/core";
import type { SamplesActions } from "../samples/samples";
import type { Views } from "../shell/views";
import { createSoundBrowserActions, type SoundBrowserActions } from "./library";
import { createSoundPlacementActions, type SoundPlacementActions } from "./soundPlacement";

export interface SoundsRole {
  readonly browser: SoundBrowserActions;
  readonly placement: SoundPlacementActions;
}

export function createSoundsRole(core: Core, views: Views, samples: Pick<SamplesActions, "importQuietly" | "sampleInfo">): SoundsRole {
  return { browser: createSoundBrowserActions(core), placement: createSoundPlacementActions(core, views, samples) };
}
