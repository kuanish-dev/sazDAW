import { t } from "../i18n";

export const MAX_PRESET_NAME_CHARS = 64;

/** What is wrong with the name (it becomes a file name); `null` if it is fine. Surrounding spaces don't count. */
export function presetNameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return t().presets.nameEmpty;
  if ([...trimmed].length > MAX_PRESET_NAME_CHARS) return t().presets.nameTooLong(MAX_PRESET_NAME_CHARS);
  // biome-ignore lint/suspicious/noControlCharactersInRegex: control characters in a file name are exactly what we forbid.
  if (trimmed.startsWith(".") || /[/\\:\u0000-\u001f\u007f]/.test(trimmed)) return t().presets.nameBadChars;
  return null;
}
