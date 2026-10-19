// The graph role: the processing graph — effects, buses, connections, faders (`graph.ts`); a moved control reports to live
// automation writing.
import { automationRole } from "../automation/role";
import { defineRole, useRole } from "../core/roles";
import { createProcessingGraph } from "./graph";

export const graphRole = defineRole("graph", ({ core, use }) => createProcessingGraph(core, use(automationRole).touch));

export const useGraph = () => useRole(graphRole);
