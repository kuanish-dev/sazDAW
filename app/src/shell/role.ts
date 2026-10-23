import { defineRole, useRole } from "../core/roles";
import { selectionRole } from "../selection/role";
import { createViews } from "./views";

export const shellRole = defineRole("shell", ({ core, use }) => createViews(use(selectionRole), core.copy.whenHas));

export const useShell = () => useRole(shellRole);
