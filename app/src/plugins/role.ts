// The plugins role: the installed third-party plugins and scanning for them (`plugins.ts`) and their native windows
// (`pluginEditor.ts`).
import { defineRole, useRole } from "../core/roles";
import { settingsRole } from "../settings/role";
import { createPluginEditorActions, type PluginEditorActions } from "./pluginEditor";
import { createPluginsActions, type PluginsActions } from "./plugins";

export type PluginsRole = PluginsActions & {
  readonly editor: PluginEditorActions;
};

export const pluginsRole = defineRole("plugins", ({ core, use }): PluginsRole => ({
  ...createPluginsActions(core, use(settingsRole).preferences),
  editor: createPluginEditorActions(core),
}));

export const usePlugins = () => useRole(pluginsRole);
