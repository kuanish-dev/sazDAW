import { For, Index, Show, createMemo, createSignal } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { TAPS_FLOOR_DB, beatTimes, delayTaps, heardTaps } from "./delayTaps";
import { pushReading, readingAt, recentPeak } from "./liveLevels";
import { dbFraction, toDb } from "../common/plot";
import { t } from "../i18n";
import { listenToNode } from "./nodeSound";
import { usePlayback } from "../core/roles";

const VIEW = { width: 300, height: 110 } as const;
/** Room under the bars for the time labels. */
const BASE = VIEW.height - 12;
const BAR_WIDTH = 7;
/** The history reaches this much past the last echo, seconds. */
const HISTORY_MARGIN_SECONDS = 0.2;
const SIDE_LABELS = { both: "", left: "L", right: "R" } as const;

/** One reading of the history (`liveLevels.ts: Reading`). */
interface InputReading {
  at: number;
  db: number;
}

export function DelayGraph(props: { node: Id; infos: ParamInfo[]; values: Params }) {
  const { tick } = usePlayback();
  const tempo = createMemo(() => tick().tempo);
  const taps = createMemo(() => delayTaps(props.infos, props.values, tempo()));
  const [history, setHistory] = createSignal<InputReading[]>([]);
  const [now, setNow] = createSignal(0);

  listenToNode(
    () => props.node,
    (sound) => {
      const keep = (taps()?.span ?? 0) + HISTORY_MARGIN_SECONDS;
      setHistory((h) => pushReading(h, { at: sound.now, db: toDb(recentPeak(sound.input, sound.sampleRate)) }, keep));
      setNow(sound.now);
    },
    () => setHistory([]),
  );

  const heard = createMemo(() => {
    const d = taps();
    if (!d) return null;
    const h = history();
    return heardTaps(d, (seconds) => readingAt(h, now() - seconds)?.db ?? -Infinity);
  });
  const x = (seconds: number) => BAR_WIDTH / 2 + 1 + (seconds / (taps()?.span ?? 1)) * (VIEW.width - BAR_WIDTH - 2);
  const height = (db: number) => dbFraction(db, TAPS_FLOOR_DB) * (BASE - 4);
  const label = () => {
    const d = taps();
    if (!d) return "";
    const ms = t().graph.effect.delayTime((d.step * 1000).toFixed(0));
    const note = props.infos.find((p) => p.name === "sync")?.choices?.[d.sync];
    return d.sync === 0 || !note ? ms : `${note} · ${ms}`;
  };

  return (
    <Show when={taps()}>
      {(d) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.delayHint}>
          <svg viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.delayGraph}>
            <For each={beatTimes(d().span, tempo())}>
              {(seconds) => <line x1={x(seconds)} x2={x(seconds)} y1="0" y2={BASE} stroke="var(--canvas-beat)" stroke-width="1" stroke-dasharray="1 3" />}
            </For>
            <line x1="0" x2={VIEW.width} y1={BASE} y2={BASE} stroke="var(--canvas-grid)" stroke-width="1" />
            <rect x={x(0) - BAR_WIDTH / 2} y={BASE - height(d().dryDb)} width={BAR_WIDTH} height={height(d().dryDb)} fill="none" stroke="var(--color-muted)" stroke-width="1" />
            <rect x={x(0) - BAR_WIDTH / 2} y={BASE - height(heard()?.dry ?? -Infinity)} width={BAR_WIDTH} height={height(heard()?.dry ?? -Infinity)} fill="var(--color-muted)" fill-opacity="0.6" />
            <Index each={d().taps}>
              {(tap, i) => (
                <>
                  <rect x={x(tap().seconds) - BAR_WIDTH / 2} y={BASE - height(tap().db)} width={BAR_WIDTH} height={height(tap().db)} fill="none" stroke="var(--color-accent)" stroke-width="1" />
                  <rect
                    x={x(tap().seconds) - BAR_WIDTH / 2}
                    y={BASE - height(heard()?.taps[i] ?? -Infinity)}
                    width={BAR_WIDTH}
                    height={height(heard()?.taps[i] ?? -Infinity)}
                    fill="var(--color-accent)"
                    fill-opacity="0.8"
                  />
                  <text x={x(tap().seconds)} y={BASE - height(tap().db) - 3} text-anchor="middle" font-size="7" fill="var(--color-muted)">{SIDE_LABELS[tap().side]}</text>
                </>
              )}
            </Index>
            <text x="2" y={VIEW.height - 2} font-size="8" fill="var(--color-muted)">0</text>
            <text x={VIEW.width - 2} y={VIEW.height - 2} text-anchor="end" font-size="8" fill="var(--color-muted)">{label()}</text>
          </svg>
        </div>
      )}
    </Show>
  );
}
