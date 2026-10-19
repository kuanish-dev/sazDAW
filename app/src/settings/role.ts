// The settings role as the session holds it: the app preferences and the look made from them, the audio devices, the
// error log and what the app says about itself.
import type { Core } from "../core/core";
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

export function createSettingsRole(core: Core): SettingsRole {
  const preferences = createPreferences(core);
  return {
    preferences,
    appearance: createAppearance(preferences.current, core.shell),
    audio: createAudioActions(core),
    log: createLogActions(core),
    about: createAboutActions(core),
  };
}
