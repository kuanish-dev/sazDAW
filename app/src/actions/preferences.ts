// App preferences: read on startup and edited with instant display (rolled back if the backend refuses).
import type { Preferences } from "../api/types";
import { t } from "../i18n";
import type { Backend, Calls, PreferencesCopy, ProjectCopy } from "./context";
import { homeDir } from "./paths";

export interface PreferencesActions {
  /** On startup: reads preferences; on failure — a toast, defaults stay. */
  load(): Promise<void>;
  /**
   * Changes preferences: the new value shows at once, then it is saved by the backend and what it stored is shown.
   * A refusal (invalid value, write failure) — a toast and the previous preferences come back.
   * A language change also re-reads the catalog: the backend writes its labels in the current language.
   */
  update(change: (current: Preferences) => Preferences): Promise<void>;
  /** Dialog to pick the default projects folder; cancel changes nothing. */
  chooseProjectsFolder(): Promise<void>;
}

export function createPreferencesActions(ctx: Backend & Calls & ProjectCopy & PreferencesCopy): PreferencesActions {
  const { api } = ctx;

  const update = async (change: (current: Preferences) => Preferences) => {
    const previous = ctx.preferences();
    ctx.showPreferences(change(previous));
    const stored = await ctx.run(api.preferences.set(ctx.preferences()));
    ctx.showPreferences(stored ?? previous);
    if (ctx.preferences().language !== previous.language) await ctx.run(ctx.refreshCatalog());
  };

  return {
    async load() {
      const stored = await ctx.run(api.preferences.get());
      if (stored) ctx.showPreferences(stored);
    },
    update,
    async chooseProjectsFolder() {
      const current = ctx.preferences().profile.projectsFolder;
      const folder = await ctx.dialogs.openDirectory({ title: t().profile.projectsFolderDialog, defaultPath: current ?? (await homeDir(api)) });
      if (folder) await update((p) => ({ ...p, profile: { ...p.profile, projectsFolder: folder } }));
    },
  };
}
