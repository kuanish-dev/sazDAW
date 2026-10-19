// The project files role: new, open, save and autosave recovery (`project.ts`), and what is made from a project —
// templates, an audio export — and what opening one leaves to say (`startup.ts`).
import { defineRole, useRole } from "../core/roles";
import { pluginsRole } from "../plugins/role";
import { settingsRole } from "../settings/role";
import { type AudioExportActions, createAudioExportActions } from "./audioExport";
import { createProjectActions, type ProjectActions } from "./project";
import { createStartupActions, type StartupActions } from "./startup";
import { createTemplatesActions, type TemplatesActions } from "./templates";

export type FilesRole = ProjectActions & {
  /** Project templates (a new project from a template is `newFromTemplate`). */
  readonly templates: TemplatesActions;
  /** Rendering the song to an audio file. */
  readonly audioExport: AudioExportActions;
  /** What the app says and asks once after the first project read. */
  readonly startup: StartupActions;
};

export const filesRole = defineRole("files", ({ core, use }): FilesRole => {
  const files = createProjectActions(core, use(settingsRole).preferences);
  return {
    ...files,
    templates: createTemplatesActions(core),
    audioExport: createAudioExportActions(core),
    startup: createStartupActions(files, use(pluginsRole)),
  };
});

export const useFiles = () => useRole(filesRole);
