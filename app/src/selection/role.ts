import { onCleanup } from "solid-js";
import { defineRole, useRole } from "../core/roles";
import { createSelection } from "./selection";

export const selectionRole = defineRole("selection", ({ core }) => {
  const selection = createSelection(core.copy.project);
  onCleanup(core.copy.onLoaded(selection.prune));
  return selection;
});

export const useSelection = () => useRole(selectionRole);
