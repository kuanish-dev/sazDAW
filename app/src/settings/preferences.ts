// App preferences: the interface's copy, which everyone reads, and everything done to it — read on startup and edited
// with instant display, rolled back if the backend refuses. Only this role changes the copy, so the instant display and
// the rollback live in one place.
import { createSignal } from "solid-js";
import type { Preferences } from "../api/types";
import type { Core } from "../core/core";
import { t } from "../i18n";
import { homeDir } from "../project/paths";
import { DEFAULT_PREFERENCES } from "./preferenceRules";

export interface AppPreferences {
  /** The current preferences: defaults until `load` brings the stored ones. */
  current(): Preferences;
  /** Shows preferences the backend already stored (another role changed them there, like a plugin's disabled flag). */
  show(stored: Preferences): void;
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

export function createPreferences(core: Core): AppPreferences {
  const { api } = core;
  const [current, setCurrent] = createSignal<Preferences>(DEFAULT_PREFERENCES);
  const show = (stored: Preferences) => void setCurrent(stored);

  const update = async (change: (current: Preferences) => Preferences) => {
    const previous = current();
    show(change(previous));
    const stored = await core.run(api.preferences.set(current()));
    show(stored ?? previous);
    if (current().language !== previous.language) await core.run(core.refreshCatalog());
  };

  return {
    current,
    show,
    async load() {
      const stored = await core.run(api.preferences.get());
      if (stored) show(stored);
    },
    update,
    async chooseProjectsFolder() {
      const folder = current().profile.projectsFolder;
      const chosen = await core.dialogs.openDirectory({ title: t().profile.projectsFolderDialog, defaultPath: folder ?? (await homeDir(api)) });
      if (chosen) await update((p) => ({ ...p, profile: { ...p.profile, projectsFolder: chosen } }));
    },
  };
}
