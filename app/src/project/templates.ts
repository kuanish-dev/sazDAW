// Project templates: list, "Save as template…" and delete. A new project from a template is created by `ProjectActions`
// (that is project file work). The name is checked before sending by the same rules as the backend (`namedSave.ts`).
import { t } from "../i18n";
import type { Core } from "../core/core";
import { saveUnderName } from "./namedSave";

export interface TemplatesActions {
  /** Template names; on error, an empty list and a notification. */
  list(): Promise<string[]>;
  /** The current project's structure as a template under the name; a taken name asks before replacing. */
  saveCurrent(name: string): Promise<boolean>;
  /** Deletes a template after confirmation. */
  remove(name: string): Promise<boolean>;
}

export function createTemplatesActions(core: Core): TemplatesActions {
  const { api, dialogs } = core;
  const list = async () => (await core.run(api.templates.list())) ?? [];

  return {
    list,
    saveCurrent(name) {
      const msg = t().presets;
      return saveUnderName(core, name, {
        title: msg.templateTitle, problem: msg.templateProblem, exists: msg.templateExists, saved: msg.templateSaved,
        taken: list,
        save: (trimmed) => api.templates.save(trimmed),
      });
    },
    async remove(name) {
      if (!(await dialogs.confirm(t().presets.templateRemove(name), t().presets.templateTitle))) return false;
      return core.attempt(api.templates.remove(name));
    },
  };
}
