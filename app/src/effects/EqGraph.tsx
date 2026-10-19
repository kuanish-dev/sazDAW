// EQ graph: the frequency response curve (20 Hz–20 kHz, ±18 dB) with draggable band handles over the live spectrum of
// the node's output (`LiveSpectrum`). Dragging sets frequency and gain, the wheel over the middle handle sets its width
// (Q), double-click resets the band; every change goes through `onChange` like a knob. An optional caption (the
// linear-phase EQ's latency) sits in the top corner. The math is in `effects/eqCurve`.
import { For, Index, Show, createMemo } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { EQ_GRID_DB, type EqHandle, HANDLE_PARAMS, dragEqHandle, eqCurvePath, eqHandles, eqSpec, gridY, wheelQ } from "./eqCurve";
import { FREQ_GRID_HZ, formatHz, freqX } from "./frequencyAxis";
import { t } from "../i18n";
import { capture, localPoint } from "../common/pointer";
import { LiveSpectrum } from "./LiveSpectrum";

const VIEW = { width: 300, height: 140 };

const LABELS: Record<EqHandle, string> = { low_cut: "LC", low: "L", mid: "M", high: "H", high_cut: "HC" };

export function EqGraph(props: { node: Id; infos: ParamInfo[]; values: Params; caption?: string; onChange: (name: string, value: number) => void }) {
  let svg!: SVGSVGElement;
  /** The handle being dragged now: the others ignore mouse movement. */
  let active: EqHandle | null = null;
  const spec = createMemo(() => eqSpec(props.infos));

  const onMove = (handle: EqHandle, event: PointerEvent) => {
    const s = spec();
    if (event.buttons !== 1 || active !== handle || !s) return;
    // The SVG stretches to the panel width: convert the screen point into its viewBox units.
    const rect = svg.getBoundingClientRect();
    const point = localPoint(svg, event);
    const x = (point.x / rect.width) * VIEW.width;
    const y = (point.y / rect.height) * VIEW.height;
    for (const update of dragEqHandle(s, handle, x, y, VIEW)) props.onChange(update.name, update.value);
  };

  const onWheel = (handle: EqHandle, event: WheelEvent) => {
    const s = spec();
    if (handle !== "mid" || !s) return;
    event.preventDefault();
    const update = wheelQ(s, props.values, event.deltaY);
    props.onChange(update.name, update.value);
  };

  const reset = (handle: EqHandle) => {
    const s = spec();
    if (!s) return;
    const { freq, gain } = HANDLE_PARAMS[handle];
    for (const name of gain ? [freq, gain] : [freq]) props.onChange(name, s.infos[name].default);
  };

  return (
    <Show when={spec()}>
      {(s) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.eqHint}>
          <svg ref={svg} viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full touch-none select-none" role="img" aria-label={t().graph.effect.eqGraph}>
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
            <path d={eqCurvePath(s(), props.values, VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
            <Show when={props.caption}>
              {(caption) => <text x={VIEW.width - 3} y="9" text-anchor="end" font-size="8" fill="var(--color-muted)">{caption()}</text>}
            </Show>
            {/* `Index`, not `For`: handle objects are rebuilt on every value change, and a recreated element would lose the pointer capture mid-drag. */}
            <Index each={eqHandles(s(), props.values, VIEW)}>
              {(h) => (
                <g
                  class="cursor-grab"
                  opacity={h().off ? 0.45 : 1}
                  onPointerDown={(e) => { active = h().id; capture(e.currentTarget, e); }}
                  onPointerUp={() => (active = null)}
                  onPointerCancel={() => (active = null)}
                  onPointerMove={(e) => onMove(h().id, e)}
                  onWheel={(e) => onWheel(h().id, e)}
                  onDblClick={() => reset(h().id)}
                >
                  <circle cx={h().x} cy={h().y} r="10" fill="transparent" />
                  <circle cx={h().x} cy={h().y} r="5.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
                  <text x={h().x} y={Math.max(11, h().y - 9)} text-anchor="middle" font-size="8" font-weight="600" fill="var(--color-muted)">{LABELS[h().id]}</text>
                </g>
              )}
            </Index>
          </svg>
        </div>
      )}
    </Show>
  );
}
