import type { KindInfo, PluginCrash, PluginInfo } from "../../api/types";
import { t } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const SYSTEM_FOLDER = "/Library/Audio/Plug-Ins/VST3";
const USER_FOLDER = "/home/user/Library/Audio/Plug-Ins/VST3";

/** A plugin param in real units (as the backend returns them); labels come from the plugin, so they are not translated. */
const param = (name: string, label: string, min: number, max: number, def: number) => ({ name, label, min, max, default: def, kind: "number" as const });

const DISTORTION: Readonly<KindInfo> = {
  kind: "vst3:distortion",
  label: "Distortion (VST3)",
  params: [
    param("p0", "Drive, dB", 0, 40, 12), param("p1", "Tone, Hz", 200, 12000, 6000), param("p2", "Mix, %", 0, 100, 100),
    { name: "p3", label: "Shape", min: 0, max: 2, default: 0, kind: "choice", choices: ["Soft (tanh)", "Hard (clip)", "Foldback"] },
  ],
} as const;

const TEST_SYNTH: Readonly<KindInfo> = {
  kind: "vst3:test_synth",
  label: "Test Synth (VST3)",
  params: [param("p0", "Cutoff, Hz", 20, 20000, 5000), param("p1", "Resonance", 0, 1, 0.2), param("p2", "Level, dB", -48, 0, -6)],
} as const;

/** What the list knows about found plugins: vendor, file and how many params the plugin itself has. */
const FOUND: readonly { info: KindInfo; type: PluginInfo["type"]; vendor: string; path: string; paramsTotal: number }[] = [
  { info: DISTORTION, type: "effect", vendor: "Mock Audio", path: `${SYSTEM_FOLDER}/Distortion.vst3`, paramsTotal: 4 },
  { info: TEST_SYNTH, type: "instrument", vendor: "Mock Audio", path: `${USER_FOLDER}/TestSynth.vst3`, paramsTotal: 12 },
] as const;
const failedScans = () => [
  { path: `${USER_FOLDER}/Broken.vst3`, reason: t().mock.plugins.broken },
  { path: `${USER_FOLDER}/Crashy.vst3`, reason: t().mock.plugins.crashed },
];

/** A crash in the previous session, to try the "Disable plugin?" window in the browser: open the page with `?pluginCrash`. */
const pretendCrash = (): PluginCrash | null =>
  typeof location !== "undefined" && new URLSearchParams(location.search).has("pluginCrash")
    ? { kind: TEST_SYNTH.kind, name: "Test Synth", project: null, places: [t().mock.plugins.crashPlace(t().mock.demo.bass)] }
    : null;

export function pluginsHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  let scanned = false;
  let crash = pretendCrash();
  const found = () => (scanned ? FOUND : []);
  const isDisabled = (kind: string) => w.preferences.disabledPlugins.includes(kind);
  const syncCatalog = () => {
    const offered = (type: PluginInfo["type"]) => found().filter((f) => f.type === type && !isDisabled(f.info.kind)).map((f) => f.info);
    w.plugins = { effects: offered("effect"), instruments: offered("instrument") };
  };
  h["plugins.scan"] = () => {
    scanned = true;
    syncCatalog();
    return { found: FOUND.length, effects: 1, instruments: 1, failed: failedScans() };
  };
  h["plugins.list"] = () => {
    const plugins = found().map((f): PluginInfo => ({
      kind: f.info.kind, name: f.info.label, vendor: f.vendor, path: f.path, type: f.type,
      params: f.info.params.length, paramsTotal: f.paramsTotal, disabled: isDisabled(f.info.kind),
    }));
    return { folders: [SYSTEM_FOLDER, USER_FOLDER], plugins };
  };
  h["plugins.setDisabled"] = ({ kind, disabled }: { kind: string; disabled: boolean }) => {
    const installed = found().some((f) => f.info.kind === kind);
    if (disabled && !installed) throw fail("PL-060", { kind });
    const others = w.preferences.disabledPlugins.filter((k) => k !== kind);
    w.preferences = { ...w.preferences, disabledPlugins: disabled ? [...others, kind] : others };
    syncCatalog();
    return null;
  };
  h["pluginEditor.open"] = () => {
    throw fail("PL-401", { details: "plugin windows exist only in the desktop app" });
  };
  h["plugins.lastCrash"] = () => crash;
  h["plugins.dismissCrash"] = () => {
    crash = null;
    return null;
  };
  return h;
}
