// Shaper graph of the distortion, saturator and tape. Left: the transfer curve (input sample → output sample, −1..1) with the
// sound now traced over it as an X-Y scope (each frame's input against its output: a pure shaper draws its own curve,
// the tone filter opens it into a loop). Right: the newest wave going in (dim) and coming out (bright), started at a
// rising zero crossing so a steady note stands still. The curve follows the knobs at once; the trace and the waves come
// from `listenToNode`, keep their last shape and fade out when the sound stops. The math is `domain/shaperCurve.ts`.
import { For, createSignal } from "solid-js";
import type { Id } from "../../api/types";
import { follow, recentPeak } from "../../domain/liveLevels";
import { TRACE_FRAMES, type Transfer, tracePath, transferPath, wavePath } from "../../domain/shaperCurve";
import { latestTraceStart } from "../../domain/visualizer";
import { t } from "../../i18n";
import { listenToNode } from "./nodeSound";

/** The curve square and, beside it after a gap, the waves: one SVG, so both share the height exactly. */
const SQUARE = { width: 140, height: 140 };
const GAP = 8;
const WAVES = { width: 160, height: 140 };
const GRID = [0.25, 0.5, 0.75];
/** Below this peak the input counts as silence: the trace keeps its last shape and fades. */
const QUIET = 0.001;
/** How fast the live parts appear and fade (time constants, s). */
const SHOW_SECONDS = 0.03;
const FADE_SECONDS = 0.3;

export function ShaperGraph(props: { node: Id; transfer: Transfer }) {
  const [trace, setTrace] = createSignal({ xy: "", input: "", output: "" });
  const [opacity, setOpacity] = createSignal(0);

  listenToNode(
    () => props.node,
    (sound) => {
      const sounding = recentPeak(sound.input, sound.sampleRate) > QUIET;
      setOpacity((o) => follow(o, sounding ? 1 : 0, sound.dt, SHOW_SECONDS, FADE_SECONDS));
      if (!sounding) return;
      const start = latestTraceStart(sound.input, TRACE_FRAMES);
      setTrace({
        xy: tracePath(sound.input, sound.output, SQUARE),
        input: wavePath(sound.input, start, TRACE_FRAMES, WAVES),
        output: wavePath(sound.output, start, TRACE_FRAMES, WAVES),
      });
    },
    () => {
      setOpacity(0);
      setTrace({ xy: "", input: "", output: "" });
    },
  );

  return (
    <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.shaperHint}>
      <svg viewBox={`0 0 ${SQUARE.width + GAP + WAVES.width} ${SQUARE.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.shaperGraph}>
        <For each={GRID}>
          {(f) => (
            <>
              <line x1={f * SQUARE.width} x2={f * SQUARE.width} y1="0" y2={SQUARE.height} stroke="var(--canvas-grid)" stroke-width={f === 0.5 ? 1.5 : 1} />
              <line x1="0" x2={SQUARE.width} y1={f * SQUARE.height} y2={f * SQUARE.height} stroke="var(--canvas-grid)" stroke-width={f === 0.5 ? 1.5 : 1} />
            </>
          )}
        </For>
        {/* 1:1 — the sound untouched. */}
        <line x1="0" y1={SQUARE.height} x2={SQUARE.width} y2="0" stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="3 3" opacity="0.6" />
        <path d={transferPath(props.transfer, SQUARE)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
        <path d={trace().xy} fill="none" stroke="var(--canvas-ink)" stroke-width="1" stroke-linejoin="round" opacity={0.7 * opacity()} />

        <g transform={`translate(${SQUARE.width + GAP} 0)`}>
          <rect width={WAVES.width} height={WAVES.height} fill="none" stroke="var(--canvas-grid)" stroke-width="1" />
          <line x1="0" x2={WAVES.width} y1={WAVES.height / 2} y2={WAVES.height / 2} stroke="var(--canvas-grid)" stroke-width="1.5" />
          <path d={trace().input} fill="none" stroke="var(--color-muted)" stroke-width="1" stroke-linejoin="round" opacity={0.8 * opacity()} />
          <path d={trace().output} fill="none" stroke="var(--color-accent)" stroke-width="1.5" stroke-linejoin="round" opacity={opacity()} />
        </g>
      </svg>
    </div>
  );
}
