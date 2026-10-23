import { defineRole, useRole } from "../core/roles";
import { createEditHistory } from "./history";

export const historyRole = defineRole("history", ({ core }) => createEditHistory(core));

export const useHistory = () => useRole(historyRole);
