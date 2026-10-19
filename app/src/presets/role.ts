// The presets role: instrument and effect presets — list, save, apply, delete (`presets.ts`).
import { defineRole, useRole } from "../core/roles";
import { createPresets } from "./presets";

export const presetsRole = defineRole("presets", ({ core }) => createPresets(core));

export const usePresets = () => useRole(presetsRole);
