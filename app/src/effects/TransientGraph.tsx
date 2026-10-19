// Transient shaper graph. Left: what the settings do to a model hit (silence, an instant rise, a fall) — the hit's level
// (dashed) and the shaped level (bright) over 400 ms, so a harder or softer start and a longer or shorter tail show at
// once while the knobs turn. Right: the last 2 seconds of the live sound as envelopes, in 5-ms steps — the input (line)
// and the output (area) — and under them how much the shaper turned the sound (above the middle line: louder, below:
// quieter). The model is `effects/transientShape.ts`; the live part comes from `listenToNode` and fades out when the
// sound stops.
import { For, createMemo, createSignal } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import { historyPoints, pushReading } from "./liveLevels";
import { type Point, areaPath, dbY, linePath } from "../common/plot";
import { MODEL_FLOOR_DB, MODEL_MS, MODEL_TOP_DB, SEGMENT_SECONDS, envelopeSegments, modelPaths, shapeModelHit } from "./transientShape";
import { t } from "../i18n";
import { listenToNode } from "./nodeSound";

/** The model square and, beside it after a gap, the live envelopes over the strip of the shaper's gain. */
const MODEL = { width: 140, height: 140 } as const;
const GAP = 8;
const LIVE = { width: 160, height: 104 } as const;
const STRIP = { top: LIVE.height + 6, height: MODEL.height - LIVE.height - 6 } as const;
const HISTORY_SECONDS = 2;
const LIVE_FLOOR_DB = -60;
/** The gain strip spans this many dB up and down. */
const STRIP_DB = 12;
const MODEL_GRID_DB = [12, 0, -12, -24, -36] as const;
const LIVE_GRID_DB = [-12, -24, -36, -48] as const;

/** One 5-ms step of the live envelopes (`liveLevels.ts: Reading`). */
interface EnvelopeReading {
  at: number;
  input: number;
  output: number;
}

export function TransientGraph(props: { node: Id; infos: ParamInfo[]; values: Params }) {
  const settings = createMemo(() => namedParams(props.infos, props.values, ["attack", "sustain"]));
  const model = createMemo(() => {
    const s = settings();
    return modelPaths(shapeModelHit((s?.attack ?? 0) / 100, (s?.sustain ?? 0) / 100), MODEL);
  });
  const [history, setHistory] = createSignal<EnvelopeReading[]>([]);
  const [now, setNow] = createSignal(0);

  listenToNode(
    () => props.node,
    (sound) => {
      const fresh = Math.round(sound.dt * sound.sampleRate);
      const input = envelopeSegments(sound.input, sound.sampleRate, fresh);
      const output = envelopeSegments(sound.output, sound.sampleRate, fresh);
      setHistory((h) =>
        input.reduce((kept, db, k) => pushReading(kept, { at: sound.now - (input.length - 1 - k) * SEGMENT_SECONDS, input: db, output: output[k] ?? db }, HISTORY_SECONDS), h),
      );
      setNow(sound.now);
    },
    () => setHistory([]),
  );

  const modelY = (db: number) => dbY(MODEL, db, MODEL_FLOOR_DB, MODEL_TOP_DB);
  const toX = (time: number) => LIVE.width * (1 - (now() - time) / HISTORY_SECONDS);
  const toY = (db: number) => dbY(LIVE, db, LIVE_FLOOR_DB);
  const stripY = (db: number) => STRIP.top + (STRIP.height / 2) * (1 - Math.max(-1, Math.min(1, db / STRIP_DB)));
  const live = createMemo(() => {
    const h = history();
    const heard = h.filter((r) => r.input > LIVE_FLOOR_DB);
    const outputArea: Point[] = h.map((r) => [toX(r.at), toY(r.output)]);
    return {
      input: linePath(historyPoints(h, (r) => r.input, toX, toY, LIVE_FLOOR_DB)),
      output: heard.length > 0 ? areaPath(outputArea, LIVE.height) : "",
      gain: linePath(h.map((r): Point | null => (r.input > LIVE_FLOOR_DB ? [toX(r.at), stripY(r.output - r.input)] : null))),
    };
  });

  return (
    <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.transientHint}>
      <svg viewBox={`0 0 ${MODEL.width + GAP + LIVE.width} ${MODEL.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.transientGraph}>
        <For each={MODEL_GRID_DB}>
          {(db) => (
            <>
              <line x1="0" x2={MODEL.width} y1={modelY(db)} y2={modelY(db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
              <text x="2" y={modelY(db) - 2} font-size="7" fill="var(--color-muted)">{db > 0 ? `+${db}` : db}</text>
            </>
          )}
        </For>
        <path d={model().input} fill="none" stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="3 3" />
        <path d={model().output} fill="none" stroke="var(--color-accent)" stroke-width="2" stroke-linejoin="round" />
        <text x={MODEL.width - 2} y={MODEL.height - 3} text-anchor="end" font-size="7" fill="var(--color-muted)">{`0…${MODEL_MS} ms`}</text>

        <g transform={`translate(${MODEL.width + GAP} 0)`}>
          <rect width={LIVE.width} height={LIVE.height} fill="none" stroke="var(--canvas-grid)" stroke-width="1" />
          <For each={LIVE_GRID_DB}>
            {(db) => <line x1="0" x2={LIVE.width} y1={toY(db)} y2={toY(db)} stroke="var(--canvas-grid)" stroke-width="1" />}
          </For>
          <path d={live().output} fill="var(--color-accent)" fill-opacity="0.3" stroke="none" />
          <path d={live().input} fill="none" stroke="var(--color-muted)" stroke-width="1.2" stroke-linejoin="round" />
          <rect y={STRIP.top} width={LIVE.width} height={STRIP.height} fill="none" stroke="var(--canvas-grid)" stroke-width="1" />
          <line x1="0" x2={LIVE.width} y1={stripY(0)} y2={stripY(0)} stroke="var(--canvas-grid)" stroke-width="1.5" />
          <path d={live().gain} fill="none" stroke="var(--color-accent)" stroke-width="1.2" stroke-linejoin="round" />
        </g>
      </svg>
    </div>
  );
}
