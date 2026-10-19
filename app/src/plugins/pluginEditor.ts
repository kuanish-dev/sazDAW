// Native plugin windows (VST3). The app owns the window; this only asks to open it.
import type { EditorTarget } from "../api/types";
import type { Core } from "../core/core";

export interface PluginEditorActions {
  /** Opens the plugin window of an effect or instrument (an open one is raised). Errors ("plugin has no window...") are notifications. */
  open(target: EditorTarget): Promise<void>;
}

export function createPluginEditorActions(core: Core): PluginEditorActions {
  return {
    open: (target) => core.exec(core.api.pluginEditor.open(target)),
  };
}
