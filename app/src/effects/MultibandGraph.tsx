// Multiband compressor graph: the band split on the frequency axis (20 Hz–20 kHz) over the live spectrum of the node's
// output (`LiveSpectrum`). Thin lines are the shapes of the bands, the bright line what all of them together do to the
// sound now (each band's gain minus its live gain reduction), and each band's reduction hangs from the top of its
// stretch with its value. Dragging a split line moves the split (double-click resets it); the S and B under each band
// toggle its solo and bypass; every change goes through `onChange` like a knob. The live reduction comes from the
// engine's band gains (`bandGains.ts`). The math is `effects/multibandCurve.ts`.
import { For, Index, Show, createMemo } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { reductionFraction } from "./dynamicsCurves";
import { FREQ_GRID_HZ, formatHz, freqX } from "./frequencyAxis";
import {
  MULTIBAND_GRID_DB,
  bandShapePath,
  bandSpan,
  bandSwitch,
  dragSplit,
  multibandCurvePath,
  multibandSettings,
  multibandY,
  playedGains,
} from "./multibandCurve";
import { t } from "../i18n";
import { capture, localPoint } from "../common/pointer";
import { createBandGains } from "./bandGains";
import { LiveSpectrum } from "./LiveSpectrum";

const VIEW = { width: 300, height: 150 };
/** How deep the reduction bars reach at full scale (`REDUCTION_SCALE_DB`): the upper part of the graph. */
const BAR_HEIGHT = VIEW.height * 0.45;
const CHIP = { width: 11, height: 10, y: VIEW.height - 13 };

export function MultibandGraph(props: { node: Id; infos: ParamInfo[]; values: Params; onChange: (name: string, value: number) => void }) {
  let svg!: SVGSVGElement;
  /** The split line being dragged now: the others ignore mouse movement. */
  let active: number | null = null;
  const settings = createMemo(() => multibandSettings(props.infos, props.values));
  const live = createBandGains(() => props.node);
  const bands = createMemo(() => {
    const s = settings();
    return s ? Array.from({ length: s.count }, (_, band) => band) : [];
  });

  const onMove = (split: number, event: PointerEvent) => {
    const s = settings();
    if (event.buttons !== 1 || active !== split || !s) return;
    // The SVG stretches to the panel width: convert the screen point into its viewBox units.
    const x = (localPoint(svg, event).x / svg.getBoundingClientRect().width) * VIEW.width;
    const update = dragSplit(s, split, x, VIEW);
    props.onChange(update.name, update.value);
  };

  return (
    <Show when={settings()}>
      {(s) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.multibandHint}>
          <svg ref={svg} viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full touch-none select-none" role="img" aria-label={t().graph.effect.multibandGraph}>
            <LiveSpectrum node={props.node} view={VIEW} />
            <For each={FREQ_GRID_HZ}>
              {(hz) => <line x1={freqX(VIEW, hz)} x2={freqX(VIEW, hz)} y1="0" y2={VIEW.height} stroke="var(--canvas-grid)" stroke-width="1" />}
            </For>
            <For each={MULTIBAND_GRID_DB}>
              {(db) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={multibandY(VIEW, db)} y2={multibandY(VIEW, db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
                  <text x="2" y={multibandY(VIEW, db) - 2} font-size="8" fill="var(--color-muted)">{db > 0 ? `+${db}` : db}</text>
                </>
              )}
            </For>
            <Index each={bands()}>
              {(band) => <path d={bandShapePath(s(), band(), VIEW)} fill="none" stroke="var(--color-muted)" stroke-width="1" opacity="0.6" />}
            </Index>
            <Index each={bands()}>
              {(band) => {
                const span = () => bandSpan(s(), band(), VIEW);
                const reduction = () => Math.max(0, -(live()[band()] ?? 0));
                return (
                  <>
                    <rect x={span().from + 1} width={Math.max(0, span().to - span().from - 2)} y="0" height={reductionFraction(reduction()) * BAR_HEIGHT} fill="var(--color-danger)" fill-opacity="0.35" />
                    <Show when={reduction() >= 0.1}>
                      <text x={(span().from + span().to) / 2} y="9" text-anchor="middle" font-size="8" fill="var(--canvas-ink)">{`−${reduction().toFixed(1)}`}</text>
                    </Show>
                    <BandChips band={band()} from={span().from} values={s().bands[band()]!} onToggle={(name, on) => props.onChange(name, on ? 1 : 0)} />
                  </>
                );
              }}
            </Index>
            <path d={multibandCurvePath(s(), playedGains(s(), live()), VIEW)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
            <Index each={s().splits.slice(0, s().count - 1)}>
              {(hz, split) => (
                <g
                  class="cursor-ew-resize"
                  onPointerDown={(e) => { active = split; capture(e.currentTarget, e); }}
                  onPointerUp={() => (active = null)}
                  onPointerCancel={() => (active = null)}
                  onPointerMove={(e) => onMove(split, e)}
                  onDblClick={() => props.onChange(s().splitInfos[split]!.name, s().splitInfos[split]!.default)}
                >
                  <line x1={freqX(VIEW, hz())} x2={freqX(VIEW, hz())} y1="0" y2={VIEW.height} stroke="transparent" stroke-width="10" />
                  <line x1={freqX(VIEW, hz())} x2={freqX(VIEW, hz())} y1="0" y2={VIEW.height} stroke="var(--color-accent)" stroke-width="1" stroke-dasharray="3 2" />
                  <circle cx={freqX(VIEW, hz())} cy={VIEW.height / 2} r="4.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
                  <text x={freqX(VIEW, hz()) + 3} y={VIEW.height / 2 - 7} font-size="7" fill="var(--color-muted)">{formatHz(hz())}</text>
                </g>
              )}
            </Index>
          </svg>
        </div>
      )}
    </Show>
  );
}

/** The band's number and its solo (S) and bypass (B) switches, at the bottom of its stretch. */
function BandChips(props: { band: number; from: number; values: { solo: boolean; bypass: boolean }; onToggle: (name: string, on: boolean) => void }) {
  const chip = (what: "solo" | "bypass", index: number, label: string, title: string) => {
    const on = () => props.values[what];
    const x = () => props.from + 10 + index * (CHIP.width + 2);
    return (
      <g class="cursor-pointer" role="button" aria-pressed={on()} onClick={() => props.onToggle(bandSwitch(props.band, what), !on())}>
        <title>{title}</title>
        <rect x={x()} y={CHIP.y} width={CHIP.width} height={CHIP.height} rx="2" fill={on() ? "var(--color-accent)" : "var(--color-bg)"} stroke="var(--color-accent)" stroke-width="1" />
        <text x={x() + CHIP.width / 2} y={CHIP.y + 7.5} text-anchor="middle" font-size="7" font-weight="600" fill={on() ? "var(--color-bg)" : "var(--color-muted)"}>{label}</text>
      </g>
    );
  };
  return (
    <>
      <text x={props.from + 3} y={CHIP.y + 7.5} font-size="7" fill="var(--color-muted)">{props.band + 1}</text>
      {chip("solo", 0, "S", t().graph.effect.bandSolo(props.band + 1))}
      {chip("bypass", 1, "B", t().graph.effect.bandBypass(props.band + 1))}
    </>
  );
}
