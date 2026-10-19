// The keyboard role: the computer keyboard as a piano on the selected track (`keyboardPiano.ts`) and the live notes it
// plays, which the piano roll's audition uses too (`live.ts`).
import { defineRole, useRole } from "../core/roles";
import { selectionRole } from "../selection/role";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboardPiano";
import { createLiveActions, type LiveActions } from "./live";

export type KeyboardRole = KeyboardPiano & {
  readonly live: LiveActions;
};

export const keyboardRole = defineRole("keyboard", ({ core, use }): KeyboardRole => {
  const live = createLiveActions(core);
  return { ...createKeyboardPiano({ selectedTrack: use(selectionRole).track, live }), live };
});

export const useKeyboard = () => useRole(keyboardRole);
