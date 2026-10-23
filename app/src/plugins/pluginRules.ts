import type { PluginCrash, PluginInfo, PluginType } from "../api/types";
import { t } from "../i18n";

/** An effect or instrument kind from a VST3 plugin (built-ins have no native window). */
export const isPluginKind = (kind: string): boolean => kind.startsWith("vst3:");

/** Plugins whose name, vendor or path contains `text` (case-insensitive), of the given type (`all` — any). */
export function filterPlugins(plugins: PluginInfo[], text: string, type: PluginType | "all"): PluginInfo[] {
  const needle = text.trim().toLowerCase();
  return plugins.filter((p) =>
    (type === "all" || p.type === type)
    && (!needle || [p.name, p.vendor, p.path].some((field) => field.toLowerCase().includes(needle))));
}

/** The last path segment: the project file name for humans. */
const fileName = (path: string): string => path.split("/").filter(Boolean).pop() ?? path;

/** Question after a crash: which plugin the previous session crashed in, where it sits, and that the last project was not opened. */
export function crashQuestion(crash: PluginCrash): string {
  return t().settings.plugins.crashQuestion(crash.name, crash.places.join(", "), crash.project ? fileName(crash.project) : null);
}
