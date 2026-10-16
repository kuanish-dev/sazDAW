// About: information about the app itself for the settings section.
import { t } from "../i18n";
import type { Backend } from "./context";

export interface AboutActions {
  /** App version; a failure doesn't break the section — it is logged (console.error) and "unknown" is shown instead. */
  version(): Promise<string>;
}

export function createAboutActions(ctx: Backend): AboutActions {
  return {
    version: () => ctx.shell.version().catch((error) => {
      console.error("version", error);
      return t().settings.log.versionUnknown;
    }),
  };
}
