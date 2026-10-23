import { defineRole, useRole } from "../core/roles";
import { createNodeSounds, type NodeSounds } from "./nodeSounds";

export interface EffectsRole {
  readonly nodeSounds: NodeSounds;
}

export const effectsRole = defineRole("effects", ({ core }): EffectsRole => ({ nodeSounds: createNodeSounds(core) }));

export const useEffects = () => useRole(effectsRole);
