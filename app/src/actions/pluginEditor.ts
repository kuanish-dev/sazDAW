// Native plugin windows (VST3). The app owns the window; this only asks to open it.
import type { EditorTarget } from "../api/types";
import type { Backend, Calls } from "./context";

export interface PluginEditorActions {
  /** Opens the plugin window of an effect or instrument (an open one is raised). Errors ("plugin has no window...") are notifications. */
  open(target: EditorTarget): Promise<void>;
}

export function createPluginEditorActions(ctx: Backend & Calls): PluginEditorActions {
  return {
    open: (target) => ctx.exec(ctx.api.pluginEditor.open(target)),
  };
}
