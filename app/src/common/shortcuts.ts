// Keyboard shortcut help: the shape of a help row and captions of physical keys (`event.code` → what's printed on the key).
// Key tables live next to their handlers (`shell/globalKeys.ts`, `tracks/clipKeys.ts`, piano roll, graph) and the help
// is built from them — so it never drifts from the behavior. Help is built on call (not at module load) so it follows the language.
import { t } from "../i18n";

/** A help row: which keys and what they do. */
export interface ShortcutHelp {
  keys: string;
  label: string;
}

const SPECIAL: Record<string, string> = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", BracketLeft: "[", BracketRight: "]",
};

/** Key caption by `event.code`: `KeyZ` → "Z", `Digit2` → "2", `Comma` → ",", `Space` → "Space" in the current language. */
export function codeLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code === "Space") return t().shortcuts.space;
  return SPECIAL[code] ?? code;
}
