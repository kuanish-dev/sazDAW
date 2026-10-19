// Dynamic EQ graph: on the EQ's axes (20 Hz–20 kHz, ±18 dB) over the live spectrum of the node's output
// (`LiveSpectrum`), the response with every band at its full gain (dashed: as far as it can go) and the response with
// the bands where they are now (bright), plus at each band a bar from 0 dB to its live gain, so a band moves with the
// sound. Each band has a handle at its frequency and full gain: dragging sets both, the wheel sets its width (Q),
// double-click resets the band; every change goes through `onChange` like a knob. The live gains are the engine's band
// gains (`bandGains.ts`). The math is `effects/dynamicEqCurve.ts`.
import { For, Index, Show, createMemo } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { type DynamicBand, bandHandle, dragBand, dynamicEqPath, dynamicEqSettings, fullGains, wheelBandQ } from "./dynamicEqCurve";
import { EQ_GRID_DB, gridY } from "./eqCurve";
import { FREQ_GRID_HZ, formatHz, freqX } from "./frequencyAxis";
import { t } from "../i18n";
import { capture, localPoint } from "../common/pointer";
import { createBandGains } from "./bandGains";
import { LiveSpectrum } from "./LiveSpectrum";

const VIEW = { width: 300, height: 140 } as const;

export function DynamicEqGraph(props: { node: Id; infos: ParamInfo[]; values: Params; onChange: (name: string, value: number) => void }) {
  let svg!: SVGSVGElement;
  /** The band being dragged now: the others ignore mouse movement. */
  let active: number | null = null;
  const bands = createMemo(() => dynamicEqSettings(props.infos, props.values));
  const live = createBandGains(() => props.node);

  const onMove = (index: number, band: DynamicBand, event: PointerEvent) => {
    if (event.buttons !== 1 || active !== index) return;
    // The SVG stretches to the panel width: convert the screen point into its viewBox units.
    const rect = svg.getBoundingClientRect();
    const point = localPoint(svg, event);
    for (const update of dragBand(band, (point.x / rect.width) * VIEW.width, (point.y / rect.height) * VIEW.height, VIEW)) {
      props.onChange(update.name, update.value);
    }
  };

  const onWheel = (band: DynamicBand, event: WheelEvent) => {
    event.preventDefault();
    const update = wheelBandQ(band, event.deltaY);
    props.onChange(update.name, update.value);
  };

  const reset = (band: DynamicBand) => {
    for (const info of [band.infos.freq, band.infos.q, band.infos.gain]) props.onChange(info.name, info.default);
  };

  return (
    <Show when={bands()}>
      {(b) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.dynamicEqHint}>
          <svg ref={svg} viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full touch-none select-none" role="img" aria-label={t().graph.effect.dynamicEqGraph}>
            <LiveSpectrum node={props.node} view={VIEW} />
            <For each={FREQ_GRID_HZ}>
              {(hz) => (
                <>
                  <line x1={freqX(VIEW, hz)} x2={freqX(VIEW, hz)} y1="0" y2={VIEW.height} stroke="var(--canvas-grid)" stroke-width="1" />
                  <text x={freqX(VIEW, hz) + 2} y={VIEW.height - 3} font-size="8" fill="var(--color-muted)">{formatHz(hz)}</text>
                </>
              )}
            </For>
            <For each={EQ_GRID_DB}>
              {(db) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={gridY(VIEW, db)} y2={gridY(VIEW, db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
                  <text x="2" y={gridY(VIEW, db) - 2} font-size="8" fill="var(--color-muted)">{db > 0 ? `+${db}` : db}</text>
                </>
              )}
            </For>
            <path d={dynamicEqPath(b(), fullGains(b()), VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="1.2" stroke-dasharray="4 3" opacity="0.6" />
            <path d={dynamicEqPath(b(), live(), VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
            {/* `Index`, not `For`: band objects are rebuilt on every value change, and a recreated element would lose the pointer capture mid-drag. */}
            <Index each={b()}>
              {(band, index) => {
                const point = () => bandHandle(band(), VIEW);
                return (
                  <>
                    <line x1={point()[0]} x2={point()[0]} y1={gridY(VIEW, 0)} y2={gridY(VIEW, live()[index] ?? 0)} stroke="var(--color-accent)" stroke-width="3" stroke-linecap="round" opacity="0.7" />
                    <g
                      class="cursor-grab"
                      opacity={band().gain === 0 ? 0.45 : 1}
                      onPointerDown={(e) => { active = index; capture(e.currentTarget, e); }}
                      onPointerUp={() => (active = null)}
                      onPointerCancel={() => (active = null)}
                      onPointerMove={(e) => onMove(index, band(), e)}
                      onWheel={(e) => onWheel(band(), e)}
                      onDblClick={() => reset(band())}
                    >
                      <circle cx={point()[0]} cy={point()[1]} r="10" fill="transparent" />
                      <circle cx={point()[0]} cy={point()[1]} r="5.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
                      <text x={point()[0]} y={Math.max(11, point()[1] - 9)} text-anchor="middle" font-size="8" font-weight="600" fill="var(--color-muted)">{index + 1}</text>
                    </g>
                  </>
                );
              }}
            </Index>
          </svg>
        </div>
      )}
    </Show>
  );
}
