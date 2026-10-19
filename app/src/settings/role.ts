// The settings role: the app preferences and the look made from them, the audio devices, the error log and what the app
// says about itself.
import { defineRole, useRole } from "../core/roles";
import { type AboutActions, createAboutActions } from "./about";
import { type Appearance, createAppearance } from "./appearance";
import { type AudioActions, createAudioActions } from "./audio";
import { createLogActions, type LogActions } from "./log";
import { type AppPreferences, createPreferences } from "./preferences";

export interface SettingsRole {
  readonly preferences: AppPreferences;
  readonly appearance: Appearance;
  readonly audio: AudioActions;
  readonly log: LogActions;
  readonly about: AboutActions;
}

export const settingsRole = defineRole("settings", ({ core }): SettingsRole => {
  const preferences = createPreferences(core);
  return {
    preferences,
    appearance: createAppearance(preferences.current, core.shell),
    audio: createAudioActions(core),
    log: createLogActions(core),
    about: createAboutActions(core),
  };
});

export const useSettings = () => useRole(settingsRole);
