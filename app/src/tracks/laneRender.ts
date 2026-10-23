import type { AutomationClip, AutomationLane } from "../api/types";
import { autoClipEnd, valueToRatio, type Range } from "../automation/automationRules";
import { clipValueAt } from "../automation/automationCurve";
import { repeatTicks } from "./clipRules";
import { clamp } from "../common/math";
import type { RowLayout } from "./rowLayout";
import { inWindow, type ScrollWindow } from "../common/scrollWindow";
import type { Palette } from "../common/theme";
import { CLIP_TITLE_PX, drawClipFrame } from "./clipFrame";
import { spanRect } from "./timelineLayout";

const BODY_ALPHA = 0.18;
/** Pixels between curve samples. */
const SAMPLE_PX = 2;
const CURVE_PAD = 4;
const REPEAT_LINE = "rgba(0,0,0,0.3)";

export interface LaneRowView {
  lane: AutomationLane;
  row: number;
  /** Clips to draw (with a drag draft applied). */
  clips: readonly AutomationClip[];
  /** The selected clip of this lane, if any. */
  selected: number | null;
  range: Range;
  label: string;
  pxPerTick: number;
  layout: RowLayout;
  window: ScrollWindow;
}

function drawClip(ctx: CanvasRenderingContext2D, v: LaneRowView, clip: AutomationClip, P: Palette) {
  const rect = spanRect(clip.startTick, clip.lengthTicks, v.row, v.pxPerTick, v.layout);
  if (!inWindow(rect.x, rect.w, v.window)) return;
  const { x, y, w, h } = rect;
  const look = { color: P.blue, name: v.label, selected: v.selected === clip.id, bodyAlpha: BODY_ALPHA, alpha: 1 };
  drawClipFrame(ctx, rect, look, P, () => {
    ctx.fillStyle = REPEAT_LINE;
    for (const tick of repeatTicks(clip.startTick, autoClipEnd(clip), clip.loopTicks)) {
      ctx.fillRect(tick * v.pxPerTick, y + CLIP_TITLE_PX, 1, h - CLIP_TITLE_PX);
    }
    const top = y + CLIP_TITLE_PX + CURVE_PAD;
    const span = Math.max(1, h - CLIP_TITLE_PX - CURVE_PAD * 2);
    const from = Math.max(x, v.window.left);
    const to = Math.min(x + w, v.window.left + v.window.width);
    ctx.strokeStyle = P.blue;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    let started = false;
    for (let px = from; px <= to; px += SAMPLE_PX) {
      const value = clipValueAt(clip, Math.min(px / v.pxPerTick, clip.startTick + clip.lengthTicks - 1));
      if (value === null) continue;
      const cy = top + (1 - clamp(valueToRatio(value, v.range), 0, 1)) * span;
      if (started) ctx.lineTo(px, cy);
      else ctx.moveTo(px, cy);
      started = true;
    }
    ctx.stroke();
    ctx.lineWidth = 1;
  });
}

export function drawLaneRow(ctx: CanvasRenderingContext2D, v: LaneRowView, P: Palette) {
  for (const clip of v.clips) drawClip(ctx, v, clip, P);
}
