// The drums role as the session holds it: building a drum part from patterns (`drums.ts`) and which sounds the pattern
// drums play, remembered between builds (`drumKit.ts`).
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import { createDrumKit, type DrumKit } from "./drumKit";
import { createDrumsActions, type DrumsActions } from "./drums";

export type DrumsRole = DrumsActions & {
  readonly kit: DrumKit;
};

export function createDrumsRole(core: Core, views: Views): DrumsRole {
  return { ...createDrumsActions(core, views), kit: createDrumKit() };
}
