import { t } from "../i18n";

/** A help row: which keys and what they do. */
export interface ShortcutHelp {
  keys: string;
  label: string;
}

const SPECIAL: Readonly<Record<string, string>> = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", BracketLeft: "[", BracketRight: "]",
} as const;

/** Key caption by `event.code`: `KeyZ` → "Z", `Digit2` → "2", `Comma` → ",", `Space` → "Space" in the current language. */
export function codeLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code === "Space") return t().shortcuts.space;
  return SPECIAL[code] ?? code;
}
