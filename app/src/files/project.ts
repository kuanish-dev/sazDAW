// Project file actions: create, open, save, restore autosave, and the warnings opening a project leaves.
// Rendering the song to audio is `files/audioExport.ts`.
import { projectFilter } from "../api/dialogs";
import type { RecentProject } from "../api/types";
import { joinPath, parentDir, projectFileFor, withExtension } from "../common/format";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { AppPreferences } from "../settings/preferences";
import { homeDir } from "./paths";

const PROJECT_EXT = ".dawproj";

export interface ProjectFiles {
  /** New project in a chosen folder; `withDemo` fills it with a ready beat, otherwise it is empty. */
  newProject(withDemo?: boolean): Promise<void>;
  /** New project in a chosen folder from a template (structure without clips). */
  newFromTemplate(template: string): Promise<void>;
  openProject(): Promise<void>;
  openRecent(path: string): Promise<void>;
  saveProject(): Promise<void>;
  saveProjectAs(): Promise<void>;
  /** On startup: what went wrong while the backend opened the last project (a missing sound…), as toasts. */
  showOpenWarnings(): Promise<void>;
  /** On startup: if an autosave was left in the project folder, offers to restore it. */
  offerAutosaveRecovery(): Promise<void>;
  /** Recent projects; an empty list on error. */
  recent(): Promise<RecentProject[]>;
}

export function createProjectFiles(core: Core, preferences: AppPreferences): ProjectFiles {
  const { api, dialogs } = core;

  /** Projects folder from preferences, otherwise "Documents". */
  const projectsFolder = async () => preferences.current().profile.projectsFolder ?? joinPath(await homeDir(api), "Documents");

  /** Where to look for projects: next to the current project's folder (the others usually live there), otherwise the projects folder. */
  const startFolder = async () => {
    const folder = core.copy.status().folder;
    return folder ? parentDir(folder) : projectsFolder();
  };

  const recent = () => api.project.recent().catch((error): RecentProject[] => { console.error("project.recent", error); return []; });

  const proceedIfSafe = async () => !core.copy.status().dirty || dialogs.confirm(t().project.unsavedQuestion, t().project.unsavedTitle);

  /** Open warnings are backend errors the backend has logged: they are only shown. */
  const showOpenWarnings = async () => {
    for (const warning of (await core.calls.run(api.project.takeWarnings())) ?? []) core.notifications.showFailure(warning);
  };

  const save = async (path: string) => {
    if (await core.calls.attempt(api.project.save(path))) core.notifications.toast("info", t().project.saved);
  };

  const saveProjectAs = async () => {
    const name = core.copy.status().name || t().project.defaultName;
    const path = await dialogs.saveFile({
      title: t().project.saveTitle,
      filters: [projectFilter()],
      defaultPath: joinPath(await startFolder(), `${name}${PROJECT_EXT}`),
    });
    if (path) await save(withExtension(path, PROJECT_EXT));
  };

  const load = async (path: string) => {
    const loaded = await core.calls.withBusy(t().project.opening, () => core.calls.attempt(api.project.load(path)));
    if (loaded) await showOpenWarnings();
    return loaded;
  };

  /** The new project's file `<folder>/<name>/<name>.dawproj` chosen by the user; `null` if they declined. */
  const chooseNewProjectFile = async () => {
    if (!(await proceedIfSafe())) return null;
    const chosen = await dialogs.saveFile({
      title: t().project.newProjectTitle,
      // A new project goes to the projects folder from preferences: that is what it is set for.
      defaultPath: joinPath(await projectsFolder(), t().project.newProjectName),
    });
    return chosen ? projectFileFor(chosen) : null;
  };

  return {
    async newProject(withDemo = false) {
      const file = await chooseNewProjectFile();
      if (!file) return;
      const created = await core.calls.withBusy(t().project.creating, async () => {
        if (!(await core.calls.attempt(api.project.create(file)))) return false;
        // The beat goes into the already created project and is saved at once, so the file isn't left empty.
        return !withDemo || ((await core.calls.attempt(api.project.fillDemo())) && (await core.calls.attempt(api.project.save(file))));
      });
      if (created) core.notifications.toast("info", t().project.created);
    },

    async newFromTemplate(template) {
      const file = await chooseNewProjectFile();
      if (!file) return;
      if (await core.calls.withBusy(t().project.creatingFromTemplate, () => core.calls.attempt(api.project.createFromTemplate(file, template)))) {
        core.notifications.toast("info", t().project.createdFromTemplate(template));
      }
    },

    async openProject() {
      if (!(await proceedIfSafe())) return;
      const path = await dialogs.openFile({ title: t().project.openTitle, filters: [projectFilter()], defaultPath: await startFolder() });
      if (path) await load(path);
    },

    async openRecent(path) {
      if (!(await proceedIfSafe())) return;
      if (!(await load(path))) await core.calls.exec(api.project.forgetRecent(path));
    },

    /** Saves to the current file; asks where if there is no file yet. */
    async saveProject() {
      const path = core.copy.status().path;
      return path ? save(path) : saveProjectAs();
    },

    saveProjectAs,
    showOpenWarnings,

    async offerAutosaveRecovery() {
      const pending = await core.calls.run(api.project.pendingAutosave());
      if (!pending) return;
      const restore = await dialogs.confirm(t().project.recoveryQuestion, t().project.recoveryTitle);
      if (!restore) return core.calls.exec(api.project.discardAutosave());
      if (await core.calls.withBusy(t().project.restoring, () => core.calls.attempt(api.project.restoreAutosave()))) await showOpenWarnings();
    },

    recent,
  };
}
