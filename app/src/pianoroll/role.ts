import { defineRole, useRole } from "../core/roles";
import { createNotes } from "./notes";

export const pianorollRole = defineRole("pianoroll", ({ core }) => createNotes(core));

export const usePianoroll = () => useRole(pianorollRole);
