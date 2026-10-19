// The settings role: the app preferences and the look made from them, the audio devices, the error log and what the app
// says about itself.
import { defineRole, useRole } from "../core/roles";
import { type About, createAbout } from "./about";
import { type Appearance, createAppearance } from "./appearance";
import { type AudioSetup, createAudioSetup } from "./audio";
import { createErrorLog, type ErrorLog } from "./log";
import { type AppPreferences, createPreferences } from "./preferences";

export interface SettingsRole {
  readonly preferences: AppPreferences;
  readonly appearance: Appearance;
  readonly audio: AudioSetup;
  readonly log: ErrorLog;
  readonly about: About;
}

export const settingsRole = defineRole("settings", ({ core }): SettingsRole => {
  const preferences = createPreferences(core);
  return {
    preferences,
    appearance: createAppearance(preferences.current, core.appShell),
    audio: createAudioSetup(core),
    log: createErrorLog(core),
    about: createAbout(core),
  };
});

export const useSettings = () => useRole(settingsRole);
