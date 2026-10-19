// What the app says and asks once, after the first project read: what went wrong opening the last project (the backend
// opens it before the window shows, if Profile says so), a plugin crash in the previous run (offer to disable the
// plugin — the backend then doesn't open the last project), and an autosave left by an abnormal exit.
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
