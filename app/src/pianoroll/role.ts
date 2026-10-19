// The piano roll role: the notes of a note clip (`notes.ts`); its editor, canvas and rules are the rest of this folder.
import { defineRole, useRole } from "../core/roles";
import { createNotesActions } from "./notes";

export const pianorollRole = defineRole("pianoroll", ({ core }) => createNotesActions(core));

export const usePianoroll = () => useRole(pianorollRole);
