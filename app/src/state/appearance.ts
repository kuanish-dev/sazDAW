// Look and language from preferences: theme (including "system", which follows the OS live), accent color,
// interface scale, track row height, hint bar and interface language. Theme and accent are `data-theme`/`data-accent`
// attributes on the page root (colors in `index.css`), scale goes through the window shell (`AppShell.setZoom`),
// language through `i18n.setLanguage`.
import { createEffect, createMemo, createSignal, on, onCleanup, type Accessor } from "solid-js";
import type { AppShell } from "../api/shell";
import type { AccentColor, Preferences } from "../api/types";
import { resolveTheme, TRACK_ROW_HEIGHTS } from "../domain/preferences";
import { isLanguage, type Language, setLanguage } from "../i18n";

/**
 * The last theme, accent and language in `localStorage` — only to avoid flicker: preferences arrive from the backend
 * asynchronously, and without the cache the first frame would always use the defaults. The source of truth is the backend.
 */
const CACHE_KEY = "daw.look";
const LIGHT_QUERY = "(prefers-color-scheme: light)";

interface Look {
  theme: "dark" | "light";
  accent: AccentColor;
  language: Language;
}

const systemLight = () => typeof matchMedia === "function" && matchMedia(LIGHT_QUERY).matches;

function applyLook(look: Look) {
  const root = document.documentElement;
  root.dataset.theme = look.theme;
  root.dataset.accent = look.accent;
  setLanguage(isLanguage(look.language) ? look.language : "en");
}

/** Before the first frame: theme, accent and language of the previous run (or the OS theme). */
export function applyCachedLook() {
  let look: Look = { theme: systemLight() ? "light" : "dark", accent: "blue", language: "en" };
  try {
    const saved = localStorage.getItem(CACHE_KEY);
    if (saved) look = { ...look, ...(JSON.parse(saved) as Partial<Look>) };
  } catch (error) {
    // The cache is only a convenience: without it the OS theme is shown until preferences arrive. Still logged.
    console.error("appearance: look cache unavailable", error);
  }
  applyLook(look);
}

function cacheLook(look: Look) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(look));
  } catch (error) {
    console.error("appearance: could not cache the look", error);
  }
}

export interface Appearance {
  /** Effective theme (with "system" resolved). */
  theme(): "dark" | "light";
  /** Track row height on the timeline, pixels. */
  rowHeight(): number;
  showHints(): boolean;
}

export function createAppearance(preferences: Accessor<Preferences>, shell: AppShell): Appearance {
  const appearance = () => preferences().appearance;

  // "System" follows the OS live, without a restart.
  const [light, setLight] = createSignal(systemLight());
  if (typeof matchMedia === "function") {
    const query = matchMedia(LIGHT_QUERY);
    const onChange = (event: MediaQueryListEvent) => setLight(event.matches);
    query.addEventListener("change", onChange);
    onCleanup(() => query.removeEventListener("change", onChange));
  }

  const theme = createMemo(() => resolveTheme(appearance().theme, light()));

  createEffect(() => {
    const look: Look = { theme: theme(), accent: appearance().accent, language: preferences().language };
    applyLook(look);
    cacheLook(look);
  });

  createEffect(on(() => appearance().uiScale, (scale) => {
    // The window keeping its old scale is no reason to break the interface, but it goes to the log.
    shell.setZoom(scale).catch((error) => console.error("setZoom", error));
  }));

  return {
    theme,
    rowHeight: () => TRACK_ROW_HEIGHTS[appearance().trackHeight],
    showHints: () => appearance().showHints,
  };
}
