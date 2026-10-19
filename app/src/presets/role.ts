// The presets role: instrument and effect presets — list, save, apply, delete (`presets.ts`).
import { defineRole, useRole } from "../core/roles";
import { createPresetsActions } from "./presets";

export const presetsRole = defineRole("presets", ({ core }) => createPresetsActions(core));

export const usePresets = () => useRole(presetsRole);
