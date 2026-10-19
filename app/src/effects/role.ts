// The effects role as the session holds it: the live sound of the nodes the effect pictures show (`nodeSounds.ts`); the
// effect editors and their graphs are components of this folder.
import type { Core } from "../core/core";
import { createNodeSounds, type NodeSounds } from "./nodeSounds";

export interface EffectsRole {
  readonly nodeSounds: NodeSounds;
}

export function createEffectsRole(core: Core): EffectsRole {
  return { nodeSounds: createNodeSounds(core) };
}
