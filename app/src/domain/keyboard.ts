/**
 * The computer keyboard as a piano, like in FL Studio. Keys are defined by physical position
 * (`event.code`), so the keyboard layout doesn't matter.
 *
 *   lower octave:  S D   G H J   L ;      black (with gaps where there are no black notes)
 *                 Z X C V B N M , . /     white
 *   upper octave:  2 3   5 6 7   9 0      black
 *                 Q W E R T Y U I O P     white
 */
import { t } from "../i18n";
import { codeLabel, type ShortcutHelp } from "./shortcuts";

const WHITE_OFFSETS = [0, 2, 4, 5, 7, 9, 11];

const LOWER_WHITE = ["KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM", "Comma", "Period", "Slash"];
const UPPER_WHITE = ["KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP"];

/** Black key semitones from the start of the row: C#, D#, F#, G#, A#, C#', D#'. */
const LOWER_BLACK: Record<string, number> = { KeyS: 1, KeyD: 3, KeyG: 6, KeyH: 8, KeyJ: 10, KeyL: 13, Semicolon: 15 };
const UPPER_BLACK: Record<string, number> = { Digit2: 1, Digit3: 3, Digit5: 6, Digit6: 8, Digit7: 10, Digit9: 13, Digit0: 15 };

function whiteSemitone(index: number): number {
  return WHITE_OFFSETS[index % 7]! + 12 * Math.floor(index / 7);
}

/** MIDI pitch for a key at base octave `octave` (C4 = 60), or `null` if the key plays no note. */
export function keyToPitch(code: string, octave: number): number | null {
  const lowerBase = (octave + 1) * 12;
  const upperBase = (octave + 2) * 12;
  let pitch: number | null = null;

  const lowerWhite = LOWER_WHITE.indexOf(code);
  const upperWhite = UPPER_WHITE.indexOf(code);
  if (lowerWhite >= 0) pitch = lowerBase + whiteSemitone(lowerWhite);
  else if (upperWhite >= 0) pitch = upperBase + whiteSemitone(upperWhite);
  else if (code in LOWER_BLACK) pitch = lowerBase + LOWER_BLACK[code]!;
  else if (code in UPPER_BLACK) pitch = upperBase + UPPER_BLACK[code]!;

  return pitch !== null && pitch >= 0 && pitch <= 127 ? pitch : null;
}

export const OCTAVE_DOWN = "BracketLeft";
export const OCTAVE_UP = "BracketRight";
export const MIN_OCTAVE = 0;
export const MAX_OCTAVE = 8;

/** Help for the Keyboard shortcuts section: built from the same rows as `keyToPitch`. */
export function keyboardPianoHelp(): readonly ShortcutHelp[] {
  const msg = t().shortcuts.piano;
  return [
    { keys: LOWER_WHITE.map(codeLabel).join(" "), label: msg.lowerWhite },
    { keys: Object.keys(LOWER_BLACK).map(codeLabel).join(" "), label: msg.lowerBlack },
    { keys: UPPER_WHITE.map(codeLabel).join(" "), label: msg.upperWhite },
    { keys: Object.keys(UPPER_BLACK).map(codeLabel).join(" "), label: msg.upperBlack },
    { keys: `${codeLabel(OCTAVE_DOWN)} ${codeLabel(OCTAVE_UP)}`, label: msg.octave(MIN_OCTAVE, MAX_OCTAVE) },
  ];
}
