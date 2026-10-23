import { For, Show, createMemo } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import {
  FILTER_GRID_DB,
  dragFilterHandle,
  filterCurvePath,
  filterGridY,
  filterHandle,
  filterSettings,
  wheelResonance,
} from "./filterCurve";
import { FREQ_GRID_HZ, formatHz, freqX } from "./frequencyAxis";
import { t } from "../i18n";
import { capture, localPoint } from "../common/pointer";
import { LiveSpectrum } from "./LiveSpectrum";

const VIEW = { width: 300, height: 140 } as const;

export function FilterGraph(props: { node: Id; infos: ParamInfo[]; values: Params; onChange: (name: string, value: number) => void }) {
  let svg!: SVGSVGElement;
  /** A drag started on the handle: a button held over it from elsewhere moves nothing. */
  let dragging = false;
  const settings = createMemo(() => filterSettings(props.infos, props.values));

  const onMove = (event: PointerEvent) => {
    const s = settings();
    if (event.buttons !== 1 || !dragging || !s) return;
    const rect = svg.getBoundingClientRect();
    const point = localPoint(svg, event);
    for (const update of dragFilterHandle(s, (point.x / rect.width) * VIEW.width, (point.y / rect.height) * VIEW.height, VIEW)) {
      props.onChange(update.name, update.value);
    }
  };

  const onWheel = (event: WheelEvent) => {
    const s = settings();
    if (!s) return;
    event.preventDefault();
    const update = wheelResonance(s, event.deltaY);
    props.onChange(update.name, update.value);
  };

  const reset = () => {
    const s = settings();
    if (!s) return;
    for (const info of [s.infos.cutoff, s.infos.resonance]) props.onChange(info.name, info.default);
  };

  return (
    <Show when={settings()}>
      {(s) => {
        const handle = () => filterHandle(s(), VIEW);
        return (
          <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.filterHint}>
            <svg ref={svg} viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full touch-none select-none" role="img" aria-label={t().graph.effect.filterGraph}>
              <LiveSpectrum node={props.node} view={VIEW} />
              <For each={FREQ_GRID_HZ}>
                {(hz) => (
                  <>
                    <line x1={freqX(VIEW, hz)} x2={freqX(VIEW, hz)} y1="0" y2={VIEW.height} stroke="var(--canvas-grid)" stroke-width="1" />
                    <text x={freqX(VIEW, hz) + 2} y={VIEW.height - 3} font-size="8" fill="var(--color-muted)">{formatHz(hz)}</text>
                  </>
                )}
              </For>
              <For each={FILTER_GRID_DB}>
                {(db) => (
                  <>
                    <line x1="0" x2={VIEW.width} y1={filterGridY(VIEW, db)} y2={filterGridY(VIEW, db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
                    <text x="2" y={filterGridY(VIEW, db) - 2} font-size="8" fill="var(--color-muted)">{db > 0 ? `+${db}` : db}</text>
                  </>
                )}
              </For>
              <path d={filterCurvePath(s(), VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
              <g
                class="cursor-grab"
                onPointerDown={(e) => { dragging = true; capture(e.currentTarget, e); }}
                onPointerUp={() => (dragging = false)}
                onPointerCancel={() => (dragging = false)}
                onPointerMove={onMove}
                onWheel={onWheel}
                onDblClick={reset}
              >
                <circle cx={handle()[0]} cy={handle()[1]} r="10" fill="transparent" />
                <circle cx={handle()[0]} cy={handle()[1]} r="5.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
              </g>
            </svg>
          </div>
        );
      }}
    </Show>
  );
}
