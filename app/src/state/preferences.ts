// The interface's copy of the app preferences: everyone reads it, only the actions layer changes it
// (`ctx.showPreferences`), so the instant display and the rollback on refusal live in one place (`actions/preferences.ts`).
import { createSignal } from "solid-js";
import type { PreferencesCopy } from "../actions";
import type { Preferences } from "../api/types";
import { DEFAULT_PREFERENCES } from "../domain/preferences";

/** Defaults until `actions.preferences.load()` brings the stored ones. */
export function createPreferencesCopy(): PreferencesCopy {
  const [preferences, showPreferences] = createSignal<Preferences>(DEFAULT_PREFERENCES);
  return { preferences, showPreferences };
}
