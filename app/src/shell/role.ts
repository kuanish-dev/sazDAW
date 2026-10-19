// The shell role: what the window shows — the tab, the settings section, the instrument window — and showing what a role
// just made once the project copy has it (`views.ts`).
import { defineRole, useRole } from "../core/roles";
import { selectionRole } from "../selection/role";
import { createViews } from "./views";

export const shellRole = defineRole("shell", ({ core, use }) => createViews(use(selectionRole), core.copy.whenHas));

export const useShell = () => useRole(shellRole);
