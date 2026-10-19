// App preferences: defaults, allowed limits (the same the backend checks), choices for
// pickers and derived values (effective theme, track row height).
import type { AccentColor, Preferences, ThemeChoice, TrackHeight } from "../api/types";
import { t } from "../i18n";

export const DEFAULT_PREFERENCES: Readonly<Preferences> = {
  language: "en",
  profile: { author: "", projectsFolder: null, openLastProject: true },
  autosaveSeconds: 30,
  appearance: { theme: "system", accent: "blue", uiScale: 1, trackHeight: "normal", showHints: true },
  disabledPlugins: [],
} as const;

/** Autosave: 0 — off, otherwise 10 seconds to 10 minutes. */
export const AUTOSAVE_OFF = 0;
export const AUTOSAVE_MIN_SECONDS = 10;
export const AUTOSAVE_MAX_SECONDS = 600;
export const AUTOSAVE_CHOICES: readonly number[] = [AUTOSAVE_OFF, 10, 30, 60, 120, 300, 600] as const;

export const UI_SCALE_MIN = 0.8;
export const UI_SCALE_MAX = 1.5;
export const UI_SCALE_STEP = 0.05;

export const THEME_CHOICES: readonly ThemeChoice[] = ["system", "dark", "light"] as const;

/** Accent presets; `swatch` is the sample for the picker button (the real theme colors live in `index.css`, `data-accent`). */
export const ACCENT_CHOICES: readonly { value: AccentColor; swatch: string }[] = [
  { value: "orange", swatch: "#ff9f43" },
  { value: "blue", swatch: "#0078d4" },
  { value: "green", swatch: "#4cd08a" },
  { value: "purple", swatch: "#b48cff" },
  { value: "red", swatch: "#ff6b81" },
] as const;

/** Track row height on the timeline, pixels: the track header (name + mixer strip) must fit. */
export const TRACK_ROW_HEIGHTS: Readonly<Record<TrackHeight, number>> = { compact: 92, normal: 120, tall: 168 } as const;

export const TRACK_HEIGHT_CHOICES: readonly TrackHeight[] = ["compact", "normal", "tall"] as const;

/** Effective theme: "system" follows the OS theme. */
export function resolveTheme(choice: ThemeChoice, systemLight: boolean): "dark" | "light" {
  if (choice === "system") return systemLight ? "light" : "dark";
  return choice;
}

/** Autosave interval label: "Off", "30 s", "2 min". */
export function autosaveLabel(seconds: number): string {
  if (seconds === AUTOSAVE_OFF) return t().appearance.autosaveOff;
  return seconds % 60 === 0 ? t().common.minutes(seconds / 60) : t().common.seconds(seconds);
}
