// The stage role: where each channel stands in the shared room, and the room (`stage.ts`); a moved place reports to live
// automation writing.
import { automationRole } from "../automation/role";
import { defineRole, useRole } from "../core/roles";
import { createStage } from "./stage";

export const stageRole = defineRole("stage", ({ core, use }) => createStage(core, use(automationRole).touch));

export const useStage = () => useRole(stageRole);
