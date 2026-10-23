import type { Plugins } from "../plugins/plugins";
import type { ProjectFiles } from "./project";

export interface Startup {
  afterFirstRead(): Promise<void>;
}

export function createStartup(
  project: Pick<ProjectFiles, "showOpenWarnings" | "offerAutosaveRecovery">, plugins: Pick<Plugins, "offerCrashReport">,
): Startup {
  return {
    async afterFirstRead() {
      await project.showOpenWarnings();
      await plugins.offerCrashReport();
      await project.offerAutosaveRecovery();
    },
  };
}
