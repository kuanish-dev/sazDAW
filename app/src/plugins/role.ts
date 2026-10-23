import { defineRole, useRole } from "../core/roles";
import { settingsRole } from "../settings/role";
import { createPluginEditor, type PluginEditor } from "./pluginEditor";
import { createPlugins, type Plugins } from "./plugins";

export type PluginsRole = Plugins & {
  readonly editor: PluginEditor;
};

export const pluginsRole = defineRole("plugins", ({ core, use }): PluginsRole => ({
  ...createPlugins(core, use(settingsRole).preferences),
  editor: createPluginEditor(core),
}));

export const usePlugins = () => useRole(pluginsRole);
