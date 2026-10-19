// Interface languages: the current language (a signal), its dictionary `t()` and number agreement `plural` (`plural.ts`).
// A leaf module: it knows nothing of the app, so every part may import it (the roles, the core, the mock, api).
// `settings/appearance.ts` sets the language from preferences; the source of truth is `Preferences.language` in the backend.
import { createSignal } from "solid-js";
import { en } from "./en";
import { ru } from "./ru";

export { plural } from "./plural";

export type { Messages } from "./en";
export type Language = "en" | "ru";

/** Languages offered in settings; each label is in its own language so anyone can recognize it. */
export const LANGUAGE_CHOICES: readonly { value: Language; label: string }[] = [
  { value: "en", label: "English" },
  { value: "ru", label: "Русский" },
] as const;

const DICTIONARIES = { en, ru } as const;

const [language, setLanguageSignal] = createSignal<Language>("en");

export { language };

/** Dictionary of the current language. Reactive: a string in JSX or a `createMemo` re-renders when the language changes. */
export const t = () => DICTIONARIES[language()];

export function isLanguage(value: unknown): value is Language {
  return value === "en" || value === "ru";
}

/** Switches the interface language live, plus `<html lang>` (screen readers, hyphenation). */
export function setLanguage(next: Language) {
  setLanguageSignal(next);
  if (typeof document !== "undefined") document.documentElement.lang = next;
}
