import { automationRole } from "../automation/role";
import { defineRole, useRole } from "../core/roles";
import { createStage } from "./stage";

export const stageRole = defineRole("stage", ({ core, use }) => createStage(core, use(automationRole).touch));

export const useStage = () => useRole(stageRole);
