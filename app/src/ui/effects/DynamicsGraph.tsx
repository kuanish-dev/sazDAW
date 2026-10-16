// Dynamics graph of the compressor, limiter and gate. Left: the static transfer curve (input level → output level) with
// the thresholds dashed and the sound now as a dot on it (input level against output level, so attack and release show
// as the dot leaving the curve). Right: the last seconds of input level (line), output level (area) and gain reduction
// (hanging from the top) against the same thresholds. Under them the gain reduction meter, and for a gate whether it is
// open. The curve follows the knobs at once; the live parts come from `listenToNode` and the engine's gain reduction and
// fade out when the sound stops. The math is `domain/dynamicsGraph.ts` with the effect's own curve module.
import { For, Index, Show, createMemo, createSignal } from "solid-js";
import type { Id } from "../../api/types";
import { CURVE_FLOOR_DB, type DynamicsCurve, HISTORY_SECONDS, curvePath, levelFraction, levelY } from "../../domain/dynamicsGraph";
import { follow, historyPoints, pushReading, recentPeak } from "../../domain/liveLevels";
import { type Point, areaPath, linePath, toDb } from "../../domain/plot";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { listenToNode } from "./nodeSound";
import { ReductionMeter } from "./ReductionMeter";

/** The curve square and, beside it after a gap, the history: one SVG, so both share the height exactly. */
const SIZE = 140;
const GAP = 8;
const HISTORY = { width: 160, height: SIZE };
const GRID_DB = [-48, -36, -24, -12];
/** The dot rises with the sound at once and sinks calmly, so it does not flicker between peaks (time constants, s). */
const DOT_RISE_SECONDS = 0.01;
const DOT_FALL_SECONDS = 0.15;
/** The dot fades over this many dB above the floor, so it disappears smoothly when the sound stops. */
const DOT_FADE_DB = 12;

/** One reading of the history (`liveLevels.ts: Reading`). */
interface LevelReading {
  at: number;
  input: number;
  output: number;
  reduction: number;
}

export function DynamicsGraph(props: { node: Id; curve: DynamicsCurve; label: string; hint: string; reduction: number }) {
  const session = useSession();
  const [history, setHistory] = createSignal<LevelReading[]>([]);
  const [now, setNow] = createSignal(0);
  const [dot, setDot] = createSignal({ input: CURVE_FLOOR_DB, output: CURVE_FLOOR_DB });
  /** The engine's latest gain reduction of this node, not held: the meter holds its peaks, the history shows the movement. */
  const rawReduction = () => session.tick().reductions.find((r) => r.node === props.node)?.db ?? 0;

  listenToNode(
    () => props.node,
    (sound) => {
      const input = Math.max(CURVE_FLOOR_DB, toDb(recentPeak(sound.input, sound.sampleRate)));
      const output = Math.max(CURVE_FLOOR_DB, toDb(recentPeak(sound.output, sound.sampleRate)));
      setHistory((h) => pushReading(h, { at: sound.now, input, output, reduction: rawReduction() }, HISTORY_SECONDS));
      setNow(sound.now);
      setDot((d) => ({
        input: follow(d.input, input, sound.dt, DOT_RISE_SECONDS, DOT_FALL_SECONDS),
        output: follow(d.output, output, sound.dt, DOT_RISE_SECONDS, DOT_FALL_SECONDS),
      }));
    },
    () => {
      setHistory([]);
      setDot({ input: CURVE_FLOOR_DB, output: CURVE_FLOOR_DB });
    },
  );

  const at = (db: number) => levelFraction(db) * SIZE;
  const toX = (time: number) => HISTORY.width * (1 - (now() - time) / HISTORY_SECONDS);
  const toY = (db: number) => levelY(HISTORY, db);
  const live = createMemo(() => {
    const h = history();
    const outputArea: Point[] = h.map((r) => [toX(r.at), toY(r.output)]);
    // Reduction hangs from the top on the same dB scale as the levels: R dB of it reaches down to the −R dB line.
    const reductionArea: Point[] = h.map((r) => [toX(r.at), toY(-r.reduction)]);
    return {
      input: linePath(historyPoints(h, (r) => r.input, toX, toY, CURVE_FLOOR_DB)),
      output: areaPath(outputArea, HISTORY.height),
      reduction: areaPath(reductionArea, 0),
    };
  });
  const dotOpacity = () => Math.min(1, (dot().input - CURVE_FLOOR_DB) / DOT_FADE_DB);
  const open = () => props.curve.isOpen?.(rawReduction());

  return (
    <div class="flex flex-col gap-2 rounded border border-line bg-bg p-2" title={props.hint}>
      <svg viewBox={`0 0 ${SIZE + GAP + HISTORY.width} ${SIZE}`} class="block h-auto w-full" role="img" aria-label={props.label}>
        <For each={GRID_DB}>
          {(db) => (
            <>
              <line x1={at(db)} x2={at(db)} y1="0" y2={SIZE} stroke="var(--canvas-grid)" stroke-width="1" />
              <line x1="0" x2={SIZE} y1={SIZE - at(db)} y2={SIZE - at(db)} stroke="var(--canvas-grid)" stroke-width="1" />
              <text x="2" y={SIZE - at(db) - 2} font-size="7" fill="var(--color-muted)">{db}</text>
            </>
          )}
        </For>
        {/* 1:1 — what an untouched signal would do. */}
        <line x1="0" y1={SIZE} x2={SIZE} y2="0" stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="3 3" opacity="0.6" />
        <For each={props.curve.thresholds}>
          {(db) => <line x1={at(db)} x2={at(db)} y1="0" y2={SIZE} stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="2 2" />}
        </For>
        <Index each={props.curve.branches}>
          {(branch) => <path d={curvePath(branch(), SIZE)} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />}
        </Index>
        <Show when={dotOpacity() > 0}>
          <circle cx={at(dot().input)} cy={SIZE - at(dot().output)} r="4" fill="var(--canvas-ink)" stroke="var(--color-accent)" stroke-width="1.5" opacity={dotOpacity()} />
        </Show>
        <text x={SIZE - 2} y={SIZE - 3} text-anchor="end" font-size="7" fill="var(--color-muted)">{`${CURVE_FLOOR_DB}…0 dB`}</text>

        <g transform={`translate(${SIZE + GAP} 0)`}>
          <rect width={HISTORY.width} height={HISTORY.height} fill="none" stroke="var(--canvas-grid)" stroke-width="1" />
          <For each={GRID_DB}>
            {(db) => <line x1="0" x2={HISTORY.width} y1={toY(db)} y2={toY(db)} stroke="var(--canvas-grid)" stroke-width="1" />}
          </For>
          <path d={live().output} fill="var(--color-accent)" fill-opacity="0.3" stroke="none" />
          <path d={live().reduction} fill="var(--color-danger)" fill-opacity="0.35" stroke="none" />
          <path d={live().input} fill="none" stroke="var(--color-muted)" stroke-width="1.2" stroke-linejoin="round" />
          <For each={props.curve.thresholds}>
            {(db) => <line x1="0" x2={HISTORY.width} y1={toY(db)} y2={toY(db)} stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="2 2" />}
          </For>
        </g>
      </svg>
      <div class="flex items-center gap-2">
        <div class="min-w-0 flex-1">
          <ReductionMeter db={props.reduction} />
        </div>
        <Show when={open() !== undefined}>
          <span class="flex shrink-0 items-center gap-1 text-[10px] text-muted" role="status">
            <span class="h-2 w-2 rounded-full" style={{ background: open() ? "var(--color-ok)" : "var(--color-panel-3)" }} />
            {open() ? t().graph.effect.gateOpen : t().graph.effect.gateClosed}
          </span>
        </Show>
      </div>
    </div>
  );
}
