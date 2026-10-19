// The project files role as the session holds it: new, open, save and autosave recovery (`project.ts`), and what is made
// from a project — templates, an audio export — and what opening one leaves to say (`startup.ts`).
import type { Core } from "../core/core";
import type { PluginsActions } from "../plugins/plugins";
import type { AppPreferences } from "../settings/preferences";
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

export function createFilesRole(core: Core, preferences: AppPreferences, plugins: Pick<PluginsActions, "offerCrashReport">): FilesRole {
  const files = createProjectActions(core, preferences);
  return {
    ...files,
    templates: createTemplatesActions(core),
    audioExport: createAudioExportActions(core),
    startup: createStartupActions(files, plugins),
  };
}
