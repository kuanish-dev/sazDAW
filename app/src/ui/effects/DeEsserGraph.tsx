// De-esser graph: on the frequency axis (20 Hz–20 kHz) over the live spectrum of the node's output (`LiveSpectrum`),
// the band it listens to (shaded), the response at its full range (dashed: as deep as it goes) and the response it
// makes now from the engine's gain reduction (bright: a dip at the band in split-band mode, the whole line down in
// wideband, the band alone while listening); under it the gain reduction meter. Dragging the band's handle sets its
// frequency, double-click resets it; the change goes through `onChange` like a knob. The math is
// `domain/deEsserCurve.ts`.
import { For, Show, createMemo } from "solid-js";
import type { Id, ParamInfo, Params } from "../../api/types";
import { DE_ESSER_GRID_DB, bandAreaPath, deEsserPath, deEsserSettings, deEsserY, dragDeEsser } from "../../domain/deEsserCurve";
import { FREQ_GRID_HZ, formatHz, freqX } from "../../domain/frequencyAxis";
import { t } from "../../i18n";
import { capture, localPoint } from "../common/pointer";
import { LiveSpectrum } from "./LiveSpectrum";
import { ReductionMeter } from "./ReductionMeter";

const VIEW = { width: 300, height: 130 };

export function DeEsserGraph(props: { node: Id; infos: ParamInfo[]; values: Params; reduction: number; onChange: (name: string, value: number) => void }) {
  let svg!: SVGSVGElement;
  /** A drag started on the handle: a button held over it from elsewhere moves nothing. */
  let dragging = false;
  const settings = createMemo(() => deEsserSettings(props.infos, props.values));

  const onMove = (event: PointerEvent) => {
    const s = settings();
    if (event.buttons !== 1 || !dragging || !s) return;
    // The SVG stretches to the panel width: convert the screen point into its viewBox units.
    const update = dragDeEsser(s, (localPoint(svg, event).x / svg.getBoundingClientRect().width) * VIEW.width, VIEW);
    props.onChange(update.name, update.value);
  };

  return (
    <Show when={settings()}>
      {(s) => (
        <div class="flex flex-col gap-2 rounded border border-line bg-bg p-2" title={t().graph.effect.deEsserHint}>
          <svg ref={svg} viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full touch-none select-none" role="img" aria-label={t().graph.effect.deEsserGraph}>
            <LiveSpectrum node={props.node} view={VIEW} />
            <For each={FREQ_GRID_HZ}>
              {(hz) => (
                <>
                  <line x1={freqX(VIEW, hz)} x2={freqX(VIEW, hz)} y1="0" y2={VIEW.height} stroke="var(--canvas-grid)" stroke-width="1" />
                  <text x={freqX(VIEW, hz) + 2} y={VIEW.height - 3} font-size="8" fill="var(--color-muted)">{formatHz(hz)}</text>
                </>
              )}
            </For>
            <For each={DE_ESSER_GRID_DB}>
              {(db) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={deEsserY(VIEW, db)} y2={deEsserY(VIEW, db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
                  <text x="2" y={deEsserY(VIEW, db) - 2} font-size="8" fill="var(--color-muted)">{db}</text>
                </>
              )}
            </For>
            <path d={bandAreaPath(s(), VIEW)} fill="var(--color-accent)" fill-opacity="0.12" stroke="none" />
            <Show when={!s().listen}>
              <path d={deEsserPath(s(), s().range, VIEW)} fill="none" stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="4 3" />
            </Show>
            <path d={deEsserPath(s(), props.reduction, VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
            <g
              class="cursor-ew-resize"
              onPointerDown={(e) => { dragging = true; capture(e.currentTarget, e); }}
              onPointerUp={() => (dragging = false)}
              onPointerCancel={() => (dragging = false)}
              onPointerMove={onMove}
              onDblClick={() => props.onChange(s().freqInfo.name, s().freqInfo.default)}
            >
              <circle cx={freqX(VIEW, s().freq)} cy={deEsserY(VIEW, 0)} r="10" fill="transparent" />
              <circle cx={freqX(VIEW, s().freq)} cy={deEsserY(VIEW, 0)} r="5.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
              <text x={freqX(VIEW, s().freq)} y={deEsserY(VIEW, 0) - 9} text-anchor="middle" font-size="8" font-weight="600" fill="var(--color-muted)">{formatHz(s().freq)}</text>
            </g>
          </svg>
          <div class="flex items-center gap-2">
            <div class="min-w-0 flex-1">
              <ReductionMeter db={props.reduction} />
            </div>
            <Show when={s().listen}>
              <span class="shrink-0 text-[10px] text-accent" role="status">{t().graph.effect.deEsserListening}</span>
            </Show>
          </div>
        </div>
      )}
    </Show>
  );
}
