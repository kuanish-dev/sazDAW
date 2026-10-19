// Phaser graph: the response of each channel (left bright, right dimmer) from 20 Hz to 20 kHz at the LFO's position now,
// over the live spectrum of the node's output (`LiveSpectrum`), with a dot at each channel's corner on the sweep between
// `low` and `high`. The notches glide as the LFO turns (while the phaser sounds, `listenToNode`) and follow the knobs at
// once. The math is `effects/phaserCurve.ts`.
import { For, Show, createMemo, createSignal } from "solid-js";
import type { Id, ParamInfo, Params } from "../api/types";
import { FREQ_GRID_HZ, formatHz, freqX } from "./frequencyAxis";
import { follow, recentPeak } from "./liveLevels";
import { PHASER_GRID_DB, type Channel, advancePhase, cornerHz, phaserPath, phaserSettings, phaserY } from "./phaserCurve";
import { dbFraction, toDb } from "../common/plot";
import { t } from "../i18n";
import { LiveSpectrum } from "./LiveSpectrum";
import { listenToNode } from "./nodeSound";

const VIEW = { width: 300, height: 140 };
const CHANNELS: { channel: Channel; color: string; opacity: number }[] = [
  { channel: 0, color: "var(--color-accent)", opacity: 1 },
  { channel: 1, color: "var(--color-blue)", opacity: 0.7 },
];
/** The dots' level scale, dB: quieter is dark. */
const GLOW_FLOOR_DB = -48;
const GLOW_RISE_SECONDS = 0.03;
const GLOW_FALL_SECONDS = 0.35;

export function PhaserGraph(props: { node: Id; infos: ParamInfo[]; values: Params }) {
  const settings = createMemo(() => phaserSettings(props.infos, props.values));
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
        <div class="rounded border border-line bg-bg p-2" title={t().graph.effect.phaserHint}>
          <svg viewBox={`0 0 ${VIEW.width} ${VIEW.height}`} class="block h-auto w-full" role="img" aria-label={t().graph.effect.phaserGraph}>
            <LiveSpectrum node={props.node} view={VIEW} />
            <For each={FREQ_GRID_HZ}>
              {(hz) => (
                <>
                  <line x1={freqX(VIEW, hz)} x2={freqX(VIEW, hz)} y1="0" y2={VIEW.height} stroke="var(--canvas-grid)" stroke-width="1" />
                  <text x={freqX(VIEW, hz) + 2} y={VIEW.height - 3} font-size="8" fill="var(--color-muted)">{formatHz(hz)}</text>
                </>
              )}
            </For>
            <For each={PHASER_GRID_DB}>
              {(db) => (
                <>
                  <line x1="0" x2={VIEW.width} y1={phaserY(VIEW, db)} y2={phaserY(VIEW, db)} stroke="var(--canvas-grid)" stroke-width={db === 0 ? 1.5 : 1} />
                  <text x="2" y={phaserY(VIEW, db) - 2} font-size="8" fill="var(--color-muted)">{db > 0 ? `+${db}` : db}</text>
                </>
              )}
            </For>
            {/* The sweep's range: the corners travel between low and high. */}
            <rect
              x={freqX(VIEW, s().low)}
              y="0"
              width={Math.max(1, freqX(VIEW, s().high) - freqX(VIEW, s().low))}
              height={VIEW.height}
              fill="var(--color-accent)"
              fill-opacity={0.04 + 0.08 * glow()}
            />
            <For each={CHANNELS}>
              {({ channel, color, opacity }) => (
                <>
                  <path d={phaserPath(s(), phase(), channel, VIEW)} fill="none" stroke={color} stroke-width="1.5" stroke-linejoin="round" opacity={opacity} />
                  <circle cx={freqX(VIEW, cornerHz(s(), phase(), channel))} cy="6" r={2.5 + 2 * glow()} fill={color} opacity={0.35 + 0.65 * glow()} />
                </>
              )}
            </For>
          </svg>
        </div>
      )}
    </Show>
  );
}
