// The keyboard role as the session holds it: the computer keyboard as a piano on the selected track
// (`keyboardPiano.ts`) and the live notes it plays, which the piano roll's audition uses too (`live.ts`).
import type { Core } from "../core/core";
import type { Selection } from "../selection/selection";
import { createKeyboardPiano, type KeyboardPiano } from "./keyboardPiano";
import { createLiveActions, type LiveActions } from "./live";

export type KeyboardRole = KeyboardPiano & {
  readonly live: LiveActions;
};

export function createKeyboardRole(core: Core, selection: Selection): KeyboardRole {
  const live = createLiveActions(core);
  return { ...createKeyboardPiano({ selectedTrack: selection.track, live }), live };
}
