// The history role: moving through the edit history — undo, redo, the list of steps, a jump to one of them.
import { defineRole, useRole } from "../core/roles";
import { createEditHistory } from "./history";

export const historyRole = defineRole("history", ({ core }) => createEditHistory(core));

export const useHistory = () => useRole(historyRole);
