import { defineRole, useRole } from "../core/roles";
import { selectionRole } from "../selection/role";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboardPiano";
import { createLiveNotes, type LiveNotes } from "./live";

export type KeyboardRole = KeyboardPiano & {
  readonly live: LiveNotes;
};

export const keyboardRole = defineRole("keyboard", ({ core, use }): KeyboardRole => {
  const live = createLiveNotes(core);
  return { ...createKeyboardPiano({ selectedTrack: use(selectionRole).track, live }), live };
});

export const useKeyboard = () => useRole(keyboardRole);
