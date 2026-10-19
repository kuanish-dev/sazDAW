// App preferences: kept in `MockWorld`, checked as the backend does (`Preferences::checked` after reading the JSON).
import type { Preferences } from "../../api/types";
import {
  ACCENT_CHOICES, AUTOSAVE_MAX_SECONDS, AUTOSAVE_MIN_SECONDS, AUTOSAVE_OFF, THEME_CHOICES, TRACK_ROW_HEIGHTS, UI_SCALE_MAX, UI_SCALE_MIN,
} from "../../settings/preferenceRules";
import { isLanguage } from "../../i18n";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

/** The backend's `MAX_AUTHOR_CHARS`. */
const MAX_AUTHOR_CHARS = 100;

function check(p: Preferences) {
  const { appearance } = p;
  // An unknown choice does not even parse in the backend.
  const unknown = [
    [isLanguage(p.language), p.language], [THEME_CHOICES.includes(appearance.theme), appearance.theme],
    [ACCENT_CHOICES.some((c) => c.value === appearance.accent), appearance.accent], [appearance.trackHeight in TRACK_ROW_HEIGHTS, appearance.trackHeight],
  ].find(([known]) => !known);
  if (unknown) throw fail("RP-101", { method: "preferences.set", details: `unknown variant \`${String(unknown[1])}\`` });
  if ([...p.profile.author.trim()].length > MAX_AUTHOR_CHARS) throw fail("PR-101", { max: MAX_AUTHOR_CHARS });
  const folder = p.profile.projectsFolder;
  if (folder && !folder.startsWith("/")) throw fail("PR-102", { path: folder });
  const seconds = p.autosaveSeconds;
  if (seconds !== AUTOSAVE_OFF && !(seconds >= AUTOSAVE_MIN_SECONDS && seconds <= AUTOSAVE_MAX_SECONDS)) {
    throw fail("PR-201", { seconds, min: AUTOSAVE_MIN_SECONDS, max: AUTOSAVE_MAX_SECONDS });
  }
  const scale = appearance.uiScale;
  if (!(scale >= UI_SCALE_MIN && scale <= UI_SCALE_MAX)) throw fail("PR-202", { scale, min: UI_SCALE_MIN, max: UI_SCALE_MAX });
}

export function preferencesHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};
  h["preferences.get"] = () => structuredClone(w.preferences);
  h["preferences.set"] = ({ preferences }: { preferences: Preferences }) => {
    check(preferences);
    // Like the backend: author name trimmed, an empty folder means "not set".
    w.preferences = structuredClone({
      ...preferences,
      profile: { ...preferences.profile, author: preferences.profile.author.trim(), projectsFolder: preferences.profile.projectsFolder || null },
    });
    return structuredClone(w.preferences);
  };
  return h;
}
