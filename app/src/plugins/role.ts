// The plugins role as the session holds it: the installed third-party plugins and scanning for them (`plugins.ts`) and
// their native windows (`pluginEditor.ts`).
import type { Core } from "../core/core";
import type { AppPreferences } from "../settings/preferences";
import { createPluginEditorActions, type PluginEditorActions } from "./pluginEditor";
import { createPluginsActions, type PluginsActions } from "./plugins";

export type PluginsRole = PluginsActions & {
  readonly editor: PluginEditorActions;
};

export function createPluginsRole(core: Core, preferences: AppPreferences): PluginsRole {
  return { ...createPluginsActions(core, preferences), editor: createPluginEditorActions(core) };
}
