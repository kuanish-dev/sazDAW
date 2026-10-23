import type { EditorTarget } from "../api/types";
import type { Core } from "../core/core";

export interface PluginEditor {
  /** Opens the plugin window of an effect or instrument (an open one is raised). Errors ("plugin has no window...") are notifications. */
  open(target: EditorTarget): Promise<void>;
}

export function createPluginEditor(core: Core): PluginEditor {
  return {
    open: (target) => core.calls.exec(core.api.pluginEditor.open(target)),
  };
}
