// The effects role: the live sound of the nodes the effect pictures show (`nodeSounds.ts`); the effect editors and their
// graphs are components of this folder.
import { defineRole, useRole } from "../core/roles";
import { createNodeSounds, type NodeSounds } from "./nodeSounds";

export interface EffectsRole {
  readonly nodeSounds: NodeSounds;
}

export const effectsRole = defineRole("effects", ({ core }): EffectsRole => ({ nodeSounds: createNodeSounds(core) }));

export const useEffects = () => useRole(effectsRole);
