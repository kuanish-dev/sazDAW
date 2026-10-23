import { For, Show, createMemo, createSignal } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { CHORUS_MAX_MS, type Voice, advancePhase, chorusSettings, delayY, sweepPath, voiceDelayMs } from "./chorusSweep";
import { follow, recentPeak } from "./liveLevels";
import { dbFraction, toDb } from "../common/plot";
import { t } from "../i18n";
import { listenToNode } from "./nodeSound";

const VIEW = { width: 300, height: 110 } as const;
const GRID_MS = [10, 20, 30, 40] as const;
const VOICES: readonly { voice: Voice; color: string }[] = [
  { voice: 0, color: "var(--color-accent)" },
  { voice: 1, color: "var(--color-blue)" },
] as const;
/** The glow's level scale, dB: quieter is dark. */
const GLOW_FLOOR_DB = -48;
const GLOW_RISE_SECONDS = 0.03;
const GLOW_FALL_SECONDS = 0.35;

export function ChorusGraph(props: { node: Id; infos: ParamInfo[]; values: Params }) {
  const settings = createMemo(() => chorusSettings(props.infos, props.values));
  const [phase, setPhase] = createSignal(0);
  const [glow, setGlow] = createSignal(0);

  listenToNode(
    () => props.node,
    (sound) => {
      const s = settings();
      if (s) setPhase((p) => advancePhase(p, s.rate, sound.dt));
      const level = dbFraction(toDb(recentPeak(sound.output, sound.sampleRate)), GLOW_FLOOR_DB);
      setGlow((g) => follow(g, level, sound.dt, GLOW_RISE_SECONDS, GLOW_FALL_SECONDS));
    },
    () => setGlow(0),
  );

  return (
    <Show when={settings()}>
      {(s) => (
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.chorusHint}>
          <svg viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.chorusGraph}>
            <For each={GRID_MS}>
              {(ms) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={delayY(VIEW, ms)} y2={delayY(VIEW, ms)} stroke="var(--canvas-grid)" stroke-width="1" />
                  <text x="2" y={delayY(VIEW, ms) - 2} font-size="7" fill="var(--color-muted)">{`${ms} ms`}</text>
                </>
              )}
            </For>
            <rect
              x="0"
              y={delayY(VIEW, s().delay + s().depth)}
              width={VIEW.width}
              height={Math.max(1, delayY(VIEW, s().delay - s().depth) - delayY(VIEW, s().delay + s().depth))}
              fill="var(--color-accent)"
              fill-opacity={0.06 + 0.14 * glow()}
            />
            <line x1="0" x2={VIEW.width} y1={delayY(VIEW, s().delay)} y2={delayY(VIEW, s().delay)} stroke="var(--color-muted)" stroke-width="1" stroke-dasharray="3 3" />
            <For each={VOICES}>
              {({ voice, color }) => (
                <>
                  <path d={sweepPath(s(), phase(), voice, VIEW)} fill="none" stroke={color} stroke-width="1.5" stroke-linejoin="round" opacity={voice === 0 ? 1 : 0.7} />
                  <circle cx={VIEW.width - 4} cy={delayY(VIEW, voiceDelayMs(s(), phase(), voice))} r={2.5 + 2.5 * glow()} fill={color} opacity={0.35 + 0.65 * glow()} />
                </>
              )}
            </For>
            <text x={VIEW.width - 2} y="9" text-anchor="end" font-size="7" fill="var(--color-muted)">{`0…${CHORUS_MAX_MS} ms`}</text>
          </svg>
        </div>
      )}
    </Show>
  );
}
