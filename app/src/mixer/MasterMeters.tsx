// The live master meter beside the master strip, the meters a mixing or mastering engineer keeps open: the integrated
// loudness with its range and how long it has gathered, momentary and short-term bars, the true peak with its held
// maximum and clip light, the phase correlation, a vectorscope, the Reset button and the mono check. While it is shown it
// polls the readings 20 times a second (`master.masterMeter`): reading keeps the backend measuring, so leaving the mixer
// stops the polling and, a few seconds later, the measuring. The scales and the picture's math are `mixer/masterMeter.ts`;
// the numbers are the backend's BS.1770 ones.
import { For, Show, createMemo, createSignal, onMount } from "solid-js";
import type { MasterReading } from "../api/types";
import {
  LOUDNESS_TARGETS, TRUE_PEAK_CEILING, correlationFraction, formatCorrelation, formatLevel, loudnessFraction, vectorPoints,
} from "./masterMeter";
import { formatDuration } from "../common/time";
import { t } from "../i18n";
import { createWatchedPoll } from "../core/poller";
import { fitCanvas } from "../common/canvas";
import { palette } from "../common/theme";
import { usePlayback } from "../core/roles";
import { useMixer } from "./role";

const POLL_MS = 50;
const SCOPE_SIZE = 176;
/** How much of the previous picture stays under the new frames: a short trail, so the dots read as a shape. */
const SCOPE_TRAIL = 0.55;
const DOT_SIZE = 1.5;
/** The picture's radius inside the canvas: room for the L/R/M/S letters. */
const SCOPE_RADIUS = SCOPE_SIZE / 2 - 12;

export function MasterMeters() {
  const { tick } = usePlayback();
  const mixer = useMixer();
  const { master } = mixer;
  const [reading, setReading] = createSignal<MasterReading | null>(null);
  const msg = () => t().mixer.meters;
  const mono = () => tick().mono;
  let scope!: HTMLCanvasElement;
  /** The theme colors, read once per theme change rather than on every frame. */
  const colors = createMemo(() => palette());

  const drawScope = (vector: number[], fresh: boolean) => {
    const P = colors();
    const ctx = fitCanvas(scope, SCOPE_SIZE, SCOPE_SIZE);
    const center = SCOPE_SIZE / 2;
    ctx.globalAlpha = fresh ? 1 : 1 - SCOPE_TRAIL;
    ctx.fillStyle = P.bgDeep;
    ctx.fillRect(0, 0, SCOPE_SIZE, SCOPE_SIZE);
    ctx.globalAlpha = 1;
    // Guides: the mid upright, the side flat, a channel alone along each diagonal.
    ctx.strokeStyle = P.grid;
    ctx.lineWidth = 1;
    ctx.beginPath();
    const reach = SCOPE_RADIUS * Math.SQRT1_2;
    ctx.moveTo(center, center - SCOPE_RADIUS);
    ctx.lineTo(center, center + SCOPE_RADIUS);
    ctx.moveTo(center - SCOPE_RADIUS, center);
    ctx.lineTo(center + SCOPE_RADIUS, center);
    ctx.moveTo(center - reach, center - reach);
    ctx.lineTo(center + reach, center + reach);
    ctx.moveTo(center + reach, center - reach);
    ctx.lineTo(center - reach, center + reach);
    ctx.stroke();
    ctx.fillStyle = P.text;
    ctx.font = `10px ${P.font}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("M", center, 6);
    ctx.fillText("L", center - reach - 6, center - reach - 4);
    ctx.fillText("R", center + reach + 6, center - reach - 4);
    ctx.fillText("S", SCOPE_SIZE - 5, center);

    const points = vectorPoints(vector);
    ctx.fillStyle = P.accent;
    for (let i = 0; i + 1 < points.length; i += 2) {
      ctx.fillRect(center + points[i]! * SCOPE_RADIUS - DOT_SIZE / 2, center - points[i + 1]! * SCOPE_RADIUS - DOT_SIZE / 2, DOT_SIZE, DOT_SIZE);
    }
  };

  const show = (next: MasterReading) => {
    setReading(next);
    drawScope(next.vector, false);
  };
  // Measured only while the mixer shows the panel; a polling failure (already shown as a toast) stops it.
  const meter = createWatchedPoll({ read: master.masterMeter, show, onStop: () => setReading(null) }, POLL_MS);
  onMount(() => {
    drawScope([], true);
    meter.start();
  });

  const peak = () => reading()?.truePeakDb ?? null;
  const maxPeak = () => reading()?.maxTruePeakDb ?? null;
  const overCeiling = (db: number | null) => db !== null && db > TRUE_PEAK_CEILING;

  return (
    <section class="flex w-52 shrink-0 flex-col gap-2 border-l border-line bg-panel p-2" aria-label={msg().title}>
      <div class="flex items-center gap-1">
        <span class="text-sm font-semibold">{msg().title}</span>
        <button
          class="btn ml-auto !h-6 !px-2 text-xs" classList={{ "btn-on": mono() }} aria-pressed={mono()}
          title={msg().monoHint} onClick={() => void master.setMono(!mono())}
        >{msg().mono}</button>
        <button class="btn !h-6 !px-2 text-xs" title={msg().resetHint} onClick={() => void master.resetMasterMeter()}>{msg().reset}</button>
      </div>

      <div class="rounded border border-field-line bg-bg px-2 py-1" title={msg().integratedHint}>
        <div class="flex items-baseline justify-between">
          <span class="label">{msg().integrated}</span>
          <span class="font-mono text-[11px] tabular-nums text-muted" title={msg().timeHint}>{formatDuration(reading()?.seconds ?? 0)}</span>
        </div>
        <div class="flex items-baseline gap-1">
          <span class="font-mono text-xl font-semibold tabular-nums text-accent">{formatLevel(reading()?.integratedLufs ?? null)}</span>
          <span class="text-xs text-muted">LUFS</span>
          <span class="ml-auto font-mono text-[11px] tabular-nums" title={msg().rangeHint}>
            {msg().range} {formatLevel(reading()?.rangeLu ?? null)} LU
          </span>
        </div>
      </div>

      <LoudnessBar label={msg().momentaryShort} title={msg().momentaryHint} value={reading()?.momentaryLufs ?? null} />
      <LoudnessBar label={msg().shortTermShort} title={msg().shortTermHint} value={reading()?.shortTermLufs ?? null} />

      <div class="flex items-center gap-1.5 text-[11px]" title={msg().truePeakHint}>
        <span class="label">{msg().truePeak}</span>
        <span class="font-mono tabular-nums" classList={{ "text-danger": overCeiling(peak()) }}>{formatLevel(peak())}</span>
        <span class="ml-auto text-muted">{msg().max}</span>
        <span class="font-mono tabular-nums" classList={{ "text-danger": overCeiling(maxPeak()) }}>{formatLevel(maxPeak())}</span>
        <button
          class="size-3 shrink-0 cursor-pointer rounded-full border border-field-line bg-panel-2"
          classList={{ "!border-danger !bg-danger shadow-[0_0_4px_var(--color-danger)]": reading()?.clipped ?? false }}
          title={reading()?.clipped ? msg().clippedHint : msg().clipHint} aria-label={msg().clip}
          onClick={() => void master.resetMasterMeter()}
        />
      </div>

      <Correlation value={reading()?.correlation ?? null} />

      <canvas ref={scope} class="block self-center rounded-sm" style={{ width: `${SCOPE_SIZE}px`, height: `${SCOPE_SIZE}px` }} title={msg().vectorscopeHint} aria-hidden="true" />
    </section>
  );
}

/** One loudness bar on the −54…0 LUFS scale with the reference targets marked, and its number. */
function LoudnessBar(props: { label: string; title: string; value: number | null }) {
  return (
    <div class="flex items-center gap-1.5" title={props.title}>
      <span class="w-3 font-mono text-[11px] text-muted">{props.label}</span>
      <div class="relative h-2 flex-1 overflow-hidden rounded-sm bg-panel-2">
        <div class="absolute inset-y-0 left-0 bg-accent transition-[width] duration-100 ease-linear" style={{ width: `${loudnessFraction(props.value) * 100}%` }} />
        <For each={LOUDNESS_TARGETS}>
          {(target) => <div class="absolute inset-y-0 w-px bg-muted" style={{ left: `${loudnessFraction(target) * 100}%` }} />}
        </For>
      </div>
      <span class="w-10 text-right font-mono text-[11px] tabular-nums">{formatLevel(props.value)}</span>
    </div>
  );
}

/** The phase correlation scale, −1 … +1, with the needle; the left half (it cancels in mono) is tinted. */
function Correlation(props: { value: number | null }) {
  const msg = () => t().mixer.meters;
  return (
    <div class="flex flex-col gap-0.5" title={msg().correlationHint}>
      <div class="flex items-baseline justify-between text-[11px]">
        <span class="label">{msg().correlation}</span>
        <span class="font-mono tabular-nums" classList={{ "text-danger": (props.value ?? 0) < 0 }}>{formatCorrelation(props.value)}</span>
      </div>
      <div class="relative h-2 rounded-sm bg-panel-2">
        <div class="absolute inset-y-0 left-0 w-1/2 rounded-l-sm bg-danger opacity-20" />
        <div class="absolute inset-y-0 left-1/2 w-px bg-muted" />
        <Show when={props.value !== null}>
          <div
            class="absolute -inset-y-0.5 w-1 -translate-x-1/2 rounded-sm transition-[left] duration-100 ease-linear"
            classList={{ "bg-danger": (props.value ?? 0) < 0, "bg-accent": (props.value ?? 0) >= 0 }}
            style={{ left: `${correlationFraction(props.value ?? 0) * 100}%` }}
          />
        </Show>
      </div>
      <div class="flex justify-between font-mono text-[10px] text-muted"><span>−1</span><span>0</span><span>+1</span></div>
    </div>
  );
}
