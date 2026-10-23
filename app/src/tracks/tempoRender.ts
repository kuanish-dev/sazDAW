import type { TempoMap } from "../api/types";
import { inWindow, type ScrollWindow } from "../common/scrollWindow";
import type { Palette } from "../common/theme";
import { FLAG_HEIGHT, FLAG_TOP, flagRect, signatureLabel } from "./tempoLayout";

/** Where the ruler is: its scale and the visible window. */
interface RulerView {
  pxPerTick: number;
  window: ScrollWindow;
}

export function drawSignatureFlags(ctx: CanvasRenderingContext2D, map: TempoMap, v: RulerView, P: Palette) {
  ctx.font = `600 10px ${P.font}`;
  ctx.textBaseline = "middle";
  map.signatures.forEach((change, i) => {
    const rect = flagRect(map, i, v.pxPerTick);
    if (!inWindow(rect.x, rect.w, v.window)) return;
    ctx.fillStyle = P.accentSoft;
    ctx.beginPath();
    ctx.roundRect(rect.x, FLAG_TOP, rect.w, FLAG_HEIGHT, 3);
    ctx.fill();
    ctx.fillStyle = P.ink;
    ctx.fillText(signatureLabel(change.signature), rect.x + 3, FLAG_TOP + FLAG_HEIGHT / 2 + 0.5);
  });
}
