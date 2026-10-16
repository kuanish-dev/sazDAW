// Reverb graph on one time line, "now" a third of the way in. Left of it, the last 2 seconds of the live sound: the dry
// input (line) and the reverb itself (area, the output minus its dry part), so its tail shows trailing the hits and dying
// away after playback stops. Right of it, how a tail dies away from now: an outline from 0 dB that follows the room and
// damping knobs at once, falling to −60 dB in the RT60 of the lows (upper edge) and of the highs (lower edge), and the
// same fan filled from the reverb's level now. The math is `domain/reverbTail.ts`.
import { For, Show, createMemo, createSignal } from "solid-js";
import type { Id, ParamInfo, Params } from "../../api/types";
import { follow, historyPoints, pushReading, recentRms } from "../../domain/liveLevels";
import { type Point, areaPath, dbY, linePath, toDb } from "../../domain/plot";
import { reverbDecay, reverbSettings, wetLevel } from "../../domain/reverbTail";
import { t } from "../../i18n";
import { listenToNode } from "./nodeSound";

const VIEW = { width: 300, height: 110 };
const FLOOR_DB = -60;
const PAST_SECONDS = 2;
const FUTURE_SECONDS = 4;
/** One scale for the past and the future: "now" sits where the past ends. */
const PX_PER_SECOND = VIEW.width / (PAST_SECONDS + FUTURE_SECONDS);
const NOW_X = PAST_SECONDS * PX_PER_SECOND;
const GRID_DB = [-12, -24, -36, -48];
const FAN_POINTS = 40;
/** The live fan starts from the reverb's level now: up at once, down calmly (time constants, s). */
const WET_RISE_SECONDS = 0.02;
const WET_FALL_SECONDS = 0.2;

/** One reading of the history (`liveLevels.ts: Reading`). */
interface TailReading {
  at: number;
  dry: number;
  wet: number;
}

/** The fan between the lows' and the highs' decay from `startDb` at "now", as a closed area. */
function fanPath(startDb: number, rt60: { low: number; high: number }): string {
  const edge = (seconds: number) =>
    Array.from({ length: FAN_POINTS + 1 }, (_, i): Point => {
      const time = (i / FAN_POINTS) * FUTURE_SECONDS;
      return [NOW_X + time * PX_PER_SECOND, dbY(VIEW, startDb - (60 * time) / seconds, FLOOR_DB)];
    });
  const upper = edge(rt60.low);
  const lower = edge(rt60.high).reverse();
  return `${linePath([...upper, ...lower])} Z`;
}

export function ReverbGraph(props: { node: Id; infos: ParamInfo[]; values: Params }) {
  const settings = createMemo(() => reverbSettings(props.infos, props.values));
  const decay = createMemo(() => {
    const s = settings();
    return s && reverbDecay(s);
  });
  const [history, setHistory] = createSignal<TailReading[]>([]);
  const [now, setNow] = createSignal(0);
  const [wetNow, setWetNow] = createSignal(FLOOR_DB);

  listenToNode(
    () => props.node,
    (sound) => {
      const s = settings();
      if (!s) return;
      const dry = Math.max(FLOOR_DB, toDb(recentRms(sound.input, sound.sampleRate)));
      const wet = Math.max(FLOOR_DB, toDb(wetLevel(sound.input, sound.output, s.mix, sound.sampleRate)));
      setHistory((h) => pushReading(h, { at: sound.now, dry, wet }, PAST_SECONDS));
      setNow(sound.now);
      setWetNow((w) => follow(w, wet, sound.dt, WET_RISE_SECONDS, WET_FALL_SECONDS));
    },
    () => {
      setHistory([]);
      setWetNow(FLOOR_DB);
    },
  );

  const toX = (time: number) => NOW_X - (now() - time) * PX_PER_SECOND;
  const toY = (db: number) => dbY(VIEW, db, FLOOR_DB);
  const past = createMemo(() => {
    const h = history();
    return {
      dry: linePath(historyPoints(h, (r) => r.dry, toX, toY, FLOOR_DB)),
      wet: areaPath(h.map((r): Point => [toX(r.at), toY(r.wet)]), VIEW.height),
    };
  });

  return (
    <Show when={decay()}>
      {(rt60) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.reverbHint}>
          <svg viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.reverbGraph}>
            <For each={GRID_DB}>
              {(db) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={toY(db)} y2={toY(db)} stroke="var(--canvas-grid)" stroke-width="1" />
                  <text x="2" y={toY(db) - 2} font-size="7" fill="var(--color-muted)">{db}</text>
                </>
              )}
            </For>
            <path d={past().wet} fill="var(--color-accent)" fill-opacity="0.35" stroke="none" />
            <path d={past().dry} fill="none" stroke="var(--color-muted)" stroke-width="1.2" stroke-linejoin="round" />
            <line x1={NOW_X} x2={NOW_X} y1="0" y2={VIEW.height} stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="2 2" />
            <path d={fanPath(0, rt60())} fill="var(--color-muted)" fill-opacity="0.1" stroke="var(--color-muted)" stroke-width="1" stroke-linejoin="round" />
            <Show when={wetNow() > FLOOR_DB + 1}>
              <path d={fanPath(wetNow(), rt60())} fill="var(--color-accent)" fill-opacity="0.35" stroke="var(--color-accent)" stroke-width="1" stroke-linejoin="round" />
            </Show>
            <text x={VIEW.width - 2} y="9" text-anchor="end" font-size="8" fill="var(--color-muted)">{t().graph.effect.rt60(rt60().low.toFixed(1))}</text>
          </svg>
        </div>
      )}
    </Show>
  );
}
