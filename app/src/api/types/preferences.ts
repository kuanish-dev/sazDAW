// App preferences (`preferences.*`) and the installed plugin list (`plugins.list`): a mirror of the backend JSON.

export type ThemeChoice = "system" | "dark" | "light";
export type AccentColor = "orange" | "blue" | "green" | "purple" | "red";
export type TrackHeight = "compact" | "normal" | "tall";
/** Interface language; the backend also writes its messages (errors, catalog labels) in it. */
export type Language = "en" | "ru";

export interface ProfilePreferences {
  /** Author name: written into MIDI export. */
  author: string;
  /** Default projects folder; `null` — not set. */
  projectsFolder: string | null;
  /** Open the last project on startup. */
  openLastProject: boolean;
}

export interface AppearancePreferences {
  theme: ThemeChoice;
  accent: AccentColor;
  /** Interface scale, 0.8…1.5. */
  uiScale: number;
  trackHeight: TrackHeight;
  /** Hint bar at the bottom of the window. */
  showHints: boolean;
}

export interface Preferences {
  language: Language;
  profile: ProfilePreferences;
  /** Autosave interval in seconds: 0 — off, otherwise 10…600. */
  autosaveSeconds: number;
  appearance: AppearancePreferences;
  /** Kinds of disabled plugins (`vst3:surge_xt`). Changed by `plugins.setDisabled`; listed here only so `preferences.set` doesn't lose them. */
  disabledPlugins: string[];
}

export type PluginType = "effect" | "instrument";

/** A discovered VST3 plugin (built-in instruments and effects are not plugins). */
export interface PluginInfo {
  kind: string;
  name: string;
  vendor: string;
  path: string;
  type: PluginType;
  /** How many parameters the DAW exposes… */
  params: number;
  /** …out of the plugin's own total. */
  paramsTotal: number;
  /** Disabled (after a crash or by hand): not offered in the catalog, projects open without it. */
  disabled: boolean;
}

export interface PluginList {
  /** Folders searched for VST3. */
  folders: string[];
  plugins: PluginInfo[];
}

/** The plugin the previous session crashed in (`plugins.lastCrash`). */
export interface PluginCrash {
  kind: string;
  name: string;
  /** The last opened project; `null` — unknown. */
  project: string | null;
  /** Where the plugin sits in that project: `track "Bass"`, `effect "Distortion"`. */
  places: string[];
}
