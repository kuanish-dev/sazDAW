import type { LibraryEntry } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";

export interface SoundLibrary {
  /** Lists a folder inside the project folder or a link; failure — a toast and `undefined`. */
  listSounds(path: string): Promise<LibraryEntry[] | undefined>;
  soundSources(): Promise<string[]>;
  /** Folder picker dialog and adding the link; returns the folder or `undefined` (cancelled/error). */
  addSoundSource(defaultPath?: string): Promise<string | undefined>;
  removeSoundSource(path: string): Promise<void>;
}

export function createSoundLibrary(core: Core): SoundLibrary {
  const { api } = core;
  return {
    listSounds: (path) => core.calls.run(api.sounds.list(path)),
    soundSources: async () => (await core.calls.run(api.sounds.sources())) ?? [],
    async addSoundSource(defaultPath) {
      const folder = await core.dialogs.openDirectory({ title: t().sounds.chooseFolderTitle, defaultPath });
      if (!folder) return undefined;
      return (await core.calls.attempt(api.sounds.addSource(folder))) ? folder : undefined;
    },
    removeSoundSource: (path) => core.calls.exec(api.sounds.removeSource(path)),
  };
}
