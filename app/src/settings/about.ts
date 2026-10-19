// About: information about the app itself for the settings section.
import { t } from "../i18n";
import type { Core } from "../core/core";

export interface About {
  /** App version; a failure doesn't break the section — it is logged (console.error) and "unknown" is shown instead. */
  version(): Promise<string>;
}

export function createAbout(core: Core): About {
  return {
    version: () => core.appShell.version().catch((error) => {
      console.error("version", error);
      return t().settings.log.versionUnknown;
    }),
  };
}
