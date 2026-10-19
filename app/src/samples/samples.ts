// Project sample catalog: which sounds the project has, their info, import and removal.
import { audioFilter } from "../api/dialogs";
import type { Id, SampleInfo } from "../api/types";
import { sampleUsage } from "./sampleRules";
import { t } from "../i18n";
import type { Core } from "../core/core";

export interface Samples {
  /** All project samples (empty if they could not be read). */
  samples(): Promise<SampleInfo[]>;
  /** Info on one sample (length, rate); `undefined` — no such sample. The list is cached until the project's samples change. */
  sampleInfo(id: Id): Promise<SampleInfo | undefined>;
  /**
   * Take a sound into work: the file is copied into the project folder (unless already there) and becomes a sample.
   * Calling again for the same file returns the same sample. Shows an "added to project" toast.
   */
  importSample(path: string): Promise<SampleInfo | undefined>;
  /** Like `importSample`, without the toast: for callers that show their own result. */
  importQuietly(path: string): Promise<SampleInfo | undefined>;
  /** File picker dialog and import of each file; returns the imported ones (empty — cancelled or errors). */
  importFiles(defaultPath?: string): Promise<SampleInfo[]>;
  /** Removes a sample from the project: if it is in use, asks first (clips with it disappear); the file stays on disk. */
  removeSample(sample: SampleInfo): Promise<void>;
}

export function createSamples(core: Core): Samples {
  const { api } = core;
  // The sample list changes only together with `project.samples`, so that is how we know the cache is stale.
  let cache: { key: string; list: Promise<SampleInfo[] | undefined> } | null = null;

  const list = async () => {
    const key = JSON.stringify(core.copy.project.samples);
    if (cache?.key !== key) cache = { key, list: core.calls.run(api.samples.list()) };
    const current = cache;
    const result = await current.list;
    // A failed answer is not cached: the next call asks again.
    if (!result && cache === current) cache = null;
    return result;
  };

  const importQuietly = (path: string) => core.calls.run(api.samples.import(path));

  return {
    samples: async () => (await list()) ?? [],

    async sampleInfo(id) {
      const found = (await list())?.find((s) => s.id === id);
      if (found) return found;
      // The project may not have been re-read after an import yet: ask the backend once more.
      cache = null;
      return (await list())?.find((s) => s.id === id);
    },

    importQuietly,

    async importSample(path) {
      const info = await importQuietly(path);
      if (info) core.notifications.toast("info", t().samples.addedToProject);
      return info;
    },

    async importFiles(defaultPath) {
      const paths = await core.dialogs.openFiles({ title: t().samples.importTitle, filters: [audioFilter()], defaultPath });
      const imported: SampleInfo[] = [];
      for (const path of paths) {
        const info = await importQuietly(path);
        if (info) imported.push(info);
      }
      if (imported.length) core.notifications.toast("info", t().samples.filesImported(imported.length));
      return imported;
    },

    async removeSample(sample) {
      const { clips, samplers } = sampleUsage(core.copy.project, sample.id);
      if (clips + samplers > 0) {
        const msg = t().samples;
        const parts = [clips > 0 ? msg.usageClips(clips) : "", samplers > 0 ? msg.usageSamplers(samplers) : ""].filter(Boolean);
        const ok = await core.dialogs.confirm(msg.confirmRemove(sample.name, parts.join(", ")), msg.removeTitle);
        if (!ok) return;
      }
      if (await core.calls.attempt(api.samples.remove(sample.id))) core.notifications.toast("info", t().samples.removed(sample.name));
    },
  };
}
