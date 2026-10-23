import { createSignal } from "solid-js";
import type { PluginList, ScanReport } from "../api/types";
import { crashQuestion } from "./pluginRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { AppPreferences } from "../settings/preferences";

export interface Plugins {
  /** Installed plugins and search folders; failure — a toast and `undefined`. */
  list(): Promise<PluginList | undefined>;
  /** Scans for plugins, refreshes the effect and instrument catalogs and reports the result; per-plugin failures go to `lastScan`. */
  scan(): Promise<void>;
  /** A scan is running (the Scan button is busy). */
  scanning(): boolean;
  /** The last scan result in this run; `null` — not scanned yet. */
  lastScan(): ScanReport | null;
  /** Disables (or enables) a plugin and re-reads the catalog; failure — a toast and `false`. */
  setDisabled(kind: string, disabled: boolean): Promise<boolean>;
  /** At startup: if the previous session died inside a plugin, asks whether to disable it; the report is then dismissed. */
  offerCrashReport(): Promise<void>;
}

export function createPlugins(core: Core, preferences: AppPreferences): Plugins {
  const [scanning, setScanning] = createSignal(false);
  const [lastScan, setLastScan] = createSignal<ScanReport | null>(null);

  const setDisabled = async (kind: string, disabled: boolean) => {
    if (!(await core.calls.attempt(core.api.plugins.setDisabled(kind, disabled)))) return false;
    await core.calls.run(core.copy.refreshCatalog());
    const stored = await core.calls.run(core.api.preferences.get());
    if (stored) preferences.show(stored);
    return true;
  };

  return {
    list: () => core.calls.run(core.api.plugins.list()),
    async scan() {
      if (scanning()) return;
      setScanning(true);
      try {
        const report = await core.calls.run(core.api.plugins.scan());
        if (!report) return;
        setLastScan(report);
        await core.calls.run(core.copy.refreshCatalog());
        core.notifications.toast("info", t().settings.plugins.scanToast(report.effects, report.instruments));
      } finally {
        setScanning(false);
      }
    },
    scanning,
    lastScan,
    setDisabled,
    async offerCrashReport() {
      const crash = await core.calls.run(core.api.plugins.lastCrash());
      if (!crash) return;
      const msg = t().settings.plugins;
      const disable = await core.dialogs.confirm(crashQuestion(crash), msg.crashTitle, { ok: msg.crashDisable, cancel: msg.crashKeep });
      if (disable && (await setDisabled(crash.kind, true))) core.notifications.toast("info", msg.crashDisabled(crash.name));
      await core.calls.exec(core.api.plugins.dismissCrash());
    },
  };
}
