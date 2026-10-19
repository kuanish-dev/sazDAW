// Installed plugins: the list, scanning for third-party ones (only the user starts it — plugins are not loaded at startup),
// disabling a plugin and the question after a plugin crash in the previous session.
// The last scan result is kept here too: the Plugins section shows it even after switching sections.
import { createSignal } from "solid-js";
import type { PluginList, ScanReport } from "../api/types";
import { crashQuestion } from "./pluginRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { AppPreferences } from "../settings/preferences";

export interface PluginsActions {
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

export function createPluginsActions(core: Core, preferences: AppPreferences): PluginsActions {
  const [scanning, setScanning] = createSignal(false);
  const [lastScan, setLastScan] = createSignal<ScanReport | null>(null);

  const setDisabled = async (kind: string, disabled: boolean) => {
    if (!(await core.attempt(core.api.plugins.setDisabled(kind, disabled)))) return false;
    await core.run(core.refreshCatalog());
    // The disabled list lives in preferences: without a fresh copy the next `preferences.set` would restore the old list.
    const stored = await core.run(core.api.preferences.get());
    if (stored) preferences.show(stored);
    return true;
  };

  return {
    list: () => core.run(core.api.plugins.list()),
    async scan() {
      if (scanning()) return;
      setScanning(true);
      try {
        const report = await core.run(core.api.plugins.scan());
        if (!report) return;
        setLastScan(report);
        await core.run(core.refreshCatalog());
        // The backend logs per-plugin failure reasons, and the Plugins section lists them.
        core.toast("info", t().settings.plugins.scanToast(report.effects, report.instruments));
      } finally {
        setScanning(false);
      }
    },
    scanning,
    lastScan,
    setDisabled,
    async offerCrashReport() {
      const crash = await core.run(core.api.plugins.lastCrash());
      if (!crash) return;
      const msg = t().settings.plugins;
      const disable = await core.dialogs.confirm(crashQuestion(crash), msg.crashTitle, { ok: msg.crashDisable, cancel: msg.crashKeep });
      if (disable && (await setDisabled(crash.kind, true))) core.toast("info", msg.crashDisabled(crash.name));
      await core.exec(core.api.plugins.dismissCrash());
    },
  };
}
