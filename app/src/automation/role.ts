// The automation role: automation lanes and their clips (`automation.ts`) and live writing, which every control that
// moves a parameter reports to (`paramTouch.ts`).
import { defineRole, useRole } from "../core/roles";
import { selectionRole } from "../selection/role";
import { shellRole } from "../shell/role";
import { type Automation, createAutomation } from "./automation";
import { createParamTouch, type ParamTouch } from "./paramTouch";

export type AutomationRole = Automation & {
  /** Live writing as the roles that move parameters see it (graph, tracks, stage). */
  readonly touch: ParamTouch;
};

export const automationRole = defineRole("automation", ({ core, use }): AutomationRole => {
  const touch = createParamTouch(core);
  return { ...createAutomation(core, use(selectionRole), use(shellRole), touch), touch };
});

export const useAutomation = () => useRole(automationRole);
