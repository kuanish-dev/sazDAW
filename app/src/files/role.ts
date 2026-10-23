import { defineRole, useRole } from "../core/roles";
import { pluginsRole } from "../plugins/role";
import { settingsRole } from "../settings/role";
import { type AudioExport, createAudioExport } from "./audioExport";
import { createProjectFiles, type ProjectFiles } from "./project";
import { createStartup, type Startup } from "./startup";
import { createTemplates, type Templates } from "./templates";

export type FilesRole = ProjectFiles & {
  /** Project templates (a new project from a template is `newFromTemplate`). */
  readonly templates: Templates;
  /** Rendering the song to an audio file. */
  readonly audioExport: AudioExport;
  /** What the app says and asks once after the first project read. */
  readonly startup: Startup;
};

export const filesRole = defineRole("files", ({ core, use }): FilesRole => {
  const files = createProjectFiles(core, use(settingsRole).preferences);
  return {
    ...files,
    templates: createTemplates(core),
    audioExport: createAudioExport(core),
    startup: createStartup(files, use(pluginsRole)),
  };
});

export const useFiles = () => useRole(filesRole);
