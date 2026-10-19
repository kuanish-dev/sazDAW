// The plugins role: the installed third-party plugins and scanning for them (`plugins.ts`) and their native windows
// (`pluginEditor.ts`).
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
