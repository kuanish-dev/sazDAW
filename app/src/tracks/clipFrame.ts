import type { Palette } from "../common/theme";
import type { PixelRect } from "./timelineLayout";

/** Corner radius (the glow layer, `activityRender.ts`, outlines clips with the same corners). */
export const CLIP_RADIUS = 4;
/** Height of the title strip with the name. */
export const CLIP_TITLE_PX = 15;
const CLIP_INK = "rgba(0,0,0,0.8)";

export interface ClipLook {
  color: string;
  name: string;
  selected: boolean;
  /** Opacity of the body tint under the content. */
  bodyAlpha: number;
  /** Opacity of the whole clip: below 1 for a muted track. */
  alpha: number;
}

/** Draws the clip frame; `content` draws inside the clipped body, with the clip's opacity set. */
export function drawClipFrame(ctx: CanvasRenderingContext2D, rect: PixelRect, look: ClipLook, P: Palette, content: () => void) {
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(rect.x, rect.y, rect.w, rect.h, CLIP_RADIUS);
  ctx.clip();
  ctx.fillStyle = look.color;
  ctx.globalAlpha = look.alpha * look.bodyAlpha;
  ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
  ctx.globalAlpha = look.alpha;
  ctx.fillRect(rect.x, rect.y, rect.w, CLIP_TITLE_PX);
  ctx.fillStyle = CLIP_INK;
  ctx.font = `600 11px ${P.font}`;
  ctx.textBaseline = "top";
  ctx.fillText(look.name, rect.x + 6, rect.y + 2);
  content();
  ctx.globalAlpha = 1;
  ctx.restore();
  ctx.beginPath();
  ctx.roundRect(rect.x + 0.5, rect.y + 0.5, rect.w - 1, rect.h - 1, CLIP_RADIUS);
  ctx.strokeStyle = look.selected ? P.ink : look.color;
  ctx.globalAlpha = look.selected ? 1 : look.alpha;
  ctx.lineWidth = look.selected ? 2 : 1;
  ctx.stroke();
  ctx.lineWidth = 1;
  ctx.globalAlpha = 1;
}
