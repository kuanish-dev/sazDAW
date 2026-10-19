// Instrument and effect presets: list a kind's presets, save a target's settings, apply a preset (one undo step), delete.
// The name is checked before sending by the same rules as the backend (`namedSave.ts`); refusals are shown as notifications.
import type { EditorTarget } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import { saveUnderName } from "../files/namedSave";

export interface PresetsActions {
  /** Preset names of a kind; on error, an empty list and a notification. */
  list(kind: string): Promise<string[]>;
  /** The target's parameters (and plugin state) as a preset of its kind; a taken name asks before replacing. */
  save(target: EditorTarget, kind: string, name: string): Promise<boolean>;
  apply(target: EditorTarget, name: string): Promise<void>;
  /** Deletes a preset of a kind after confirmation. */
  remove(kind: string, name: string): Promise<boolean>;
}

export function createPresetsActions(core: Core): PresetsActions {
  const { api, dialogs } = core;
  const list = async (kind: string) => (await core.run(api.presets.list(kind))) ?? [];

  return {
    list,
    save(target, kind, name) {
      const msg = t().presets;
      return saveUnderName(core, name, {
        title: msg.presetTitle, problem: msg.presetProblem, exists: msg.presetExists, saved: msg.presetSaved,
        taken: () => list(kind),
        save: (trimmed) => api.presets.save(target, trimmed),
      });
    },
    apply: (target, name) => core.exec(api.presets.apply(target, name)),
    async remove(kind, name) {
      if (!(await dialogs.confirm(t().presets.presetRemove(name), t().presets.presetTitle))) return false;
      return core.attempt(api.presets.remove(kind, name));
    },
  };
}
