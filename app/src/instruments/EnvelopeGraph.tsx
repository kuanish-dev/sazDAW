import { For, Show, createMemo } from "solid-js";
import type { ParamInfo } from "../api/types";
import { dragEnvelope, envelopeSpec, layoutEnvelope, type Handle } from "./envelope";
import { formatParam } from "../common/format";
import { t } from "../i18n";
import { capture, localPoint } from "../common/pointer";
import { sticky } from "../common/sticky";

const WIDTH = 300;
const HEIGHT = 124;

const LABELS: Readonly<Record<Handle, string>> = { attack: "A", hold: "H", decay: "D", sustain: "S", release: "R" } as const;

export function EnvelopeGraph(props: {
  infos: ParamInfo[];
  values: Record<string, number>;
  onChange: (name: string, value: number) => void;
}) {
  let svg!: SVGSVGElement;
  /** The handle being dragged now: the others ignore mouse movement. */
  let active: Handle | null = null;
  const spec = createMemo(() => envelopeSpec(props.infos));
  const layout = createMemo(() => {
    const s = spec();
    return s ? layoutEnvelope(props.values, s, WIDTH, HEIGHT) : null;
  });
  const lastLayout = sticky(layout);

  const handles = createMemo(() => {
    const l = layout();
    if (!l) return [];
    const list: { id: Handle; x: number; y: number }[] = [{ id: "attack", ...l.attack }];
    if (l.hold) list.push({ id: "hold", ...l.hold });
    if (l.decay) list.push({ id: "decay", ...l.decay });
    if (spec()?.sustain) list.push({ id: "sustain", ...l.sustain });
    list.push({ id: "release", ...l.release });
    return list;
  });

  const onMove = (handle: Handle, event: PointerEvent) => {
    if (event.buttons !== 1 || active !== handle) return;
    const s = spec();
    const l = layout();
    if (!s || !l) return;
    const rect = svg.getBoundingClientRect();
    const point = localPoint(svg, event);
    const x = (point.x / rect.width) * WIDTH;
    const y = (point.y / rect.height) * HEIGHT;
    for (const update of dragEnvelope(handle, x, y, l, s)) props.onChange(update.name, update.value);
  };

  const readout = (name: string, suffix = "") => {
    const value = props.values[name];
    return value === undefined ? null : `${formatParam(value)}${suffix}`;
  };

  return (
    <Show when={layout()}>
      <div class="rounded border border-line bg-bg p-2">
        <svg ref={svg} viewBox={`0 0 ${WIDTH} ${HEIGHT}`} class="block h-auto w-full touch-none" role="img" aria-label={t().tracks.envelope.graph}>
          <defs>
            <linearGradient id="envelope-fill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stop-color="var(--color-accent)" stop-opacity="0.55" />
              <stop offset="100%" stop-color="var(--color-accent)" stop-opacity="0.05" />
            </linearGradient>
          </defs>
          <For each={[0.25, 0.5, 0.75]}>
            {(f) => <line x1="0" x2={WIDTH} y1={lastLayout()!.top + (lastLayout()!.bottom - lastLayout()!.top) * f} y2={lastLayout()!.top + (lastLayout()!.bottom - lastLayout()!.top) * f} stroke="var(--canvas-grid)" stroke-width="1" />}
          </For>
          <path d={`${lastLayout()!.path} L${lastLayout()!.release.x.toFixed(1)} ${lastLayout()!.bottom} L${lastLayout()!.start.x} ${lastLayout()!.bottom} Z`} fill="url(#envelope-fill)" />
          <path d={lastLayout()!.path} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
          <For each={handles()}>
            {(h) => (
              <g
                class="cursor-grab"
                onPointerDown={(e) => { active = h.id; capture(e.currentTarget, e); }}
                onPointerUp={() => (active = null)}
                onPointerCancel={() => (active = null)}
                onPointerMove={(e) => onMove(h.id, e)}
                onDblClick={() => {
                  const info = props.infos.find((p) => p.name === h.id);
                  if (info) props.onChange(info.name, info.default);
                }}
              >
                <circle cx={h.x} cy={h.y} r="9" fill="transparent" />
                <circle cx={h.x} cy={h.y} r="5.5" fill="var(--color-bg)" stroke="var(--color-accent)" stroke-width="2" />
                <text x={h.x} y={Math.max(11, h.y - 9)} text-anchor="middle" font-size="9" font-weight="600" fill="var(--color-muted)">{LABELS[h.id]}</text>
              </g>
            )}
          </For>
        </svg>
        <div class="mt-1 grid grid-cols-5 gap-1 whitespace-nowrap text-center font-mono text-[10px] text-muted" title={t().tracks.envelope.hint}>
          <span>A {readout("attack")}</span>
          <span>{spec()?.hold ? `H ${readout("hold")}` : ""}</span>
          <span>{spec()?.decay ? `D ${readout("decay")}` : ""}</span>
          <span>{spec()?.sustain ? `S ${Math.round((props.values.sustain ?? 0) * 100)}%` : ""}</span>
          <span>R {readout("release")}</span>
        </div>
      </div>
    </Show>
  );
}
