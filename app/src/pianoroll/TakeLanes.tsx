import { For, Show, createSignal } from "solid-js";
import type { Clip, Id } from "../api/types";
import { compOf, draggedRange, takeRanges, takesOf } from "../tracks/takes";
import { FREE_GRID } from "../common/time";
import { t } from "../i18n";
import { IconClose } from "../common/icons";
import { capture, localPoint } from "../common/pointer";
import { useClipWaves } from "../tracks/clipWaves";
import { useProjectCopy } from "../core/roles";
import { useTracks } from "../tracks/role";

/** Row waveform width in `viewBox` units (the SVG is stretched to the panel width). */
const WAVE_WIDTH = 600;
const ROW_HEIGHT = 34;

/** A drag along a take row: from which clip tick to which. */
interface Drag {
  take: Id;
  anchor: number;
  current: number;
  free: boolean;
}

export function TakeLanes(props: { clip: Clip; snap: number }) {
  const { project } = useProjectCopy();
  const tracks = useTracks();
  const { clips } = tracks;
  const waves = useClipWaves();
  const [drag, setDrag] = createSignal<Drag | null>(null);
  const takes = () => takesOf(props.clip.content);
  const comped = () => compOf(props.clip.content).length > 0;
  const active = () => (props.clip.content.type === "audio" ? props.clip.content.sample : null);
  const ranges = () => takeRanges(props.clip.content, props.clip.lengthTicks);
  const name = (sample: Id) => project.samples.find((s) => s.id === sample)?.name ?? t().pianoRoll.takeSound(sample);
  const x = (tick: number) => (tick / props.clip.lengthTicks) * WAVE_WIDTH;

  const tickAt = (element: Element, event: PointerEvent) => {
    const share = localPoint(element, event).x / Math.max(1, element.getBoundingClientRect().width);
    return Math.min(1, Math.max(0, share)) * props.clip.lengthTicks;
  };
  const range = (d: Drag) => draggedRange(project.tempoMap, props.clip, d.anchor, d.current, d.free ? FREE_GRID : props.snap);

  const wavePath = (take: Id) => {
    const columns = waves.takeColumns(props.clip, take, WAVE_WIDTH, project.tempoMap);
    if (!columns) return "";
    const mid = ROW_HEIGHT / 2;
    const scale = mid - 2;
    return columns.map(([low, high], i) => `M${i + 0.5} ${(mid - high * scale).toFixed(1)}V${(mid - low * scale + 0.5).toFixed(1)}`).join("");
  };

  return (
    <Show when={takes().length > 1}>
      <fieldset class="flex min-w-0 flex-col gap-1">
        <legend class="label mb-1">{t().pianoRoll.takes}{comped() ? t().pianoRoll.comped : ""}</legend>
        <p class="text-xs text-muted">{t().pianoRoll.takesHint}</p>
        <For each={takes()}>
          {(take, index) => (
            <div class="flex items-center gap-2">
              <button
                class="btn !h-7 w-48 shrink-0 justify-start gap-2 text-left text-xs"
                classList={{ "btn-on": !comped() && take === active() }}
                title={t().pianoRoll.wholeTakeHint}
                onClick={() => void clips.setActiveTake(props.clip.id, take)}
              >
                <span class="w-4 shrink-0 font-mono text-muted">{index() + 1}</span>
                <span class="truncate">{name(take)}</span>
              </button>
              <svg
                viewBox={`0 0 ${WAVE_WIDTH} ${ROW_HEIGHT}`} preserveAspectRatio="none"
                class="block min-w-0 flex-1 cursor-crosshair touch-none rounded border border-line bg-bg" style={{ height: `${ROW_HEIGHT}px` }}
                role="img" aria-label={t().pianoRoll.takeAria(index() + 1)}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  capture(e.currentTarget, e);
                  const tick = tickAt(e.currentTarget, e);
                  setDrag({ take, anchor: tick, current: tick, free: e.altKey });
                }}
                onPointerMove={(e) => {
                  const d = drag();
                  if (d?.take === take) setDrag({ ...d, current: tickAt(e.currentTarget, e), free: e.altKey });
                }}
                onPointerUp={() => {
                  const d = drag();
                  setDrag(null);
                  const chosen = d && range(d);
                  if (d && chosen) void clips.chooseTakeRange(props.clip.id, chosen.from, chosen.to, d.take);
                }}
                onPointerCancel={() => setDrag(null)}
              >
                <For each={ranges().filter((r) => r.take === take)}>
                  {(r) => <rect x={x(r.from)} y="0" width={x(r.to) - x(r.from)} height={ROW_HEIGHT} fill="var(--color-accent)" fill-opacity="0.22" />}
                </For>
                <path d={wavePath(take)} stroke="var(--color-text)" stroke-opacity="0.65" stroke-width="1" vector-effect="non-scaling-stroke" />
                <Show when={drag()?.take === take && range(drag()!)}>
                  {(r) => (
                    <rect x={x(r().from)} y="1" width={x(r().to) - x(r().from)} height={ROW_HEIGHT - 2}
                      fill="var(--color-accent)" fill-opacity="0.35" stroke="var(--color-accent)" vector-effect="non-scaling-stroke" />
                  )}
                </Show>
              </svg>
              <button
                class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger"
                title={t().pianoRoll.removeTakeHint} aria-label={t().pianoRoll.removeTakeAria(index() + 1)}
                onClick={() => void clips.removeTake(props.clip.id, take)}
              ><IconClose size={14} /></button>
            </div>
          )}
        </For>
      </fieldset>
    </Show>
  );
}
