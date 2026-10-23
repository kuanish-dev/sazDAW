import { defineRole, useRole } from "../core/roles";
import { shellRole } from "../shell/role";
import { createChords } from "./chords";

export const chordsRole = defineRole("chords", ({ core, use }) => createChords(core, use(shellRole)));

export const useChords = () => useRole(chordsRole);
