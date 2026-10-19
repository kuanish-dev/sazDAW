// The selection role: what is selected — the track, clips, a sample, automation (`selection.ts`). It gives way to items a
// new project copy no longer has.
import { onCleanup } from "solid-js";
import { defineRole, useRole } from "../core/roles";
import { createSelection } from "./selection";

export const selectionRole = defineRole("selection", ({ core }) => {
  const selection = createSelection(core.project);
  onCleanup(core.onLoaded(selection.prune));
  return selection;
});

export const useSelection = () => useRole(selectionRole);
