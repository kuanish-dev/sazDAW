// Signature flags on the bar ruler ("7/8" right after the bar number): geometry shared by drawing (`tempoRender.ts`)
// and mouse (`tempoPointer.ts`), like `markerLayout.ts` for markers.
import type { TempoMap } from "../api/types";
import { barIndex } from "../common/tempoMap";

/** Signature flag: in the top row of the ruler, right after the bar number. */
export const FLAG_TOP = 2;
export const FLAG_HEIGHT = 11;
/** Width of a bar-number digit and of a flag character (11px and 10px fonts; estimated without a canvas so mouse and drawing agree). */
const DIGIT_PX = 6.5;
const FLAG_CHAR_PX = 6;
const FLAG_PAD = 6;

export const signatureLabel = (s: { numerator: number; denominator: number }) => `${s.numerator}/${s.denominator}`;

/** Rectangle of the signature flag for change `index` (x from the timeline start). */
export function flagRect(map: TempoMap, index: number, pxPerTick: number): { x: number; w: number } {
  const change = map.signatures[index]!;
  const number = String(barIndex(map, change.tick) + 1);
  const x = change.tick * pxPerTick + 4 + number.length * DIGIT_PX + 3;
  return { x, w: FLAG_PAD + signatureLabel(change.signature).length * FLAG_CHAR_PX };
}

/** Signature change whose flag contains (x, y); `null` if none. */
export function flagAt(map: TempoMap, x: number, y: number, pxPerTick: number): number | null {
  if (y < FLAG_TOP || y > FLAG_TOP + FLAG_HEIGHT) return null;
  for (let i = map.signatures.length - 1; i >= 0; i--) {
    const rect = flagRect(map, i, pxPerTick);
    if (x >= rect.x && x <= rect.x + rect.w) return i;
  }
  return null;
}
