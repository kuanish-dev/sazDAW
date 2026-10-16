// Overview strip of the whole file under the waveform: a frame shows the main waveform's window and can be dragged.
import { createEffect } from "solid-js";
import type { Waveform } from "../../api/types";
import { viewFractions, type Selection, type View } from "../../domain/waveform";
import { t } from "../../i18n";
import { fitCanvas } from "../common/canvas";
import { capture, localPoint } from "../common/pointer";
import { palette } from "../common/theme";

const HEIGHT = 46;
const MIN_WIDTH = 100;
/** Waveform padding from the strip's top and bottom, pixels. */
const WAVE_PAD = 3;
const FRAME_WIDTH = 1.5;

export function Overview(props: {
  wave: Waveform | null;
  total: number;
  view: View;
  selection: Selection | null;
  onPan: (start: number) => void;
}) {
  let canvas!: HTMLCanvasElement;
  let host!: HTMLDivElement;
  let dragOffset: number | null = null;

  const draw = () => {
    const P = palette();
    const w = Math.max(MIN_WIDTH, Math.floor(host.clientWidth));
    const ctx = fitCanvas(canvas, w, HEIGHT);
    ctx.fillStyle = P.bg;
    ctx.fillRect(0, 0, w, HEIGHT);

    const wave = props.wave;
    const mid = HEIGHT / 2;
    if (wave) {
      ctx.fillStyle = P.waveDim;
      const barWidth = Math.max(1, w / wave.peaks.length);
      wave.peaks.forEach(([low, high], i) => {
        ctx.fillRect((i / wave.peaks.length) * w, mid - high * (mid - WAVE_PAD), barWidth, Math.max(1, (high - low) * (mid - WAVE_PAD)));
      });
    }
    if (props.selection) {
      ctx.fillStyle = P.accentSoft;
      const x0 = (props.selection.from / props.total) * w;
      ctx.fillRect(x0, 0, ((props.selection.to - props.selection.from) / props.total) * w, HEIGHT);
    }
    const { from, to } = viewFractions(props.view, props.total);
    ctx.strokeStyle = P.ink;
    ctx.lineWidth = FRAME_WIDTH;
    ctx.strokeRect(from * w + FRAME_WIDTH / 2, FRAME_WIDTH / 2, Math.max(3, (to - from) * w) - FRAME_WIDTH, HEIGHT - FRAME_WIDTH);
    ctx.lineWidth = 1;
  };
  createEffect(draw);

  const frameAt = (event: PointerEvent) => (localPoint(canvas, event).x / canvas.clientWidth) * props.total;

  return (
    <div ref={host} class="w-full overflow-hidden rounded border border-line" title={t().samples.overviewHint}>
      <canvas
        ref={canvas}
        class="block cursor-grab touch-none"
        onPointerDown={(e) => {
          capture(canvas, e);
          const centre = frameAt(e);
          const inside = centre >= props.view.start && centre <= props.view.start + props.view.span;
          // A press inside the frame holds it by that spot; outside, it centres the frame on the point.
          dragOffset = inside ? centre - props.view.start : props.view.span / 2;
          props.onPan(centre - dragOffset);
        }}
        onPointerMove={(e) => dragOffset !== null && props.onPan(frameAt(e) - dragOffset)}
        onPointerUp={() => (dragOffset = null)}
      />
    </div>
  );
}
