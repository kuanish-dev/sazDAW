// The channel utility controls of a strip (polarity, width, mono, fine time alignment): neutral values, ranges, units.
// The engine side is `daw-engine/src/mixer/utility.rs`; the delay in samples is rounded the same way it rounds.
import { MAX_CHANNEL_DELAY_MS, MAX_WIDTH, type Polarity, type Strip } from "../api/types";
import { t } from "../i18n";
import { clamp } from "../common/math";

export type ChannelUtility = Pick<Strip, "polarity" | "width" | "mono" | "delayMs">;

/** A new channel's utility, as `Strip::default()` in Rust (a fresh object: the caller may change it). */
export function neutralUtility(): ChannelUtility {
  return { polarity: { invertLeft: false, invertRight: false }, width: 1, mono: false, delayMs: 0 };
}

/** Knob ranges: the width from mono to twice the side, the delay either way. */
export const WIDTH_RANGE = { min: 0, max: MAX_WIDTH, default: 1 } as const;
export const DELAY_RANGE = { min: -MAX_CHANNEL_DELAY_MS, max: MAX_CHANNEL_DELAY_MS, default: 0 } as const;

/** The Ø button: both sides on turns both off, anything else turns both on. */
export function flipBoth(polarity: Polarity): Polarity {
  const on = !(polarity.invertLeft && polarity.invertRight);
  return { invertLeft: on, invertRight: on };
}

/** The delay in whole samples at the stream rate, as the engine plays it. */
export function delaySamples(ms: number, sampleRate: number): number {
  return Math.round((ms * sampleRate) / 1000);
}

/** The delay `samples` samples later (negative: earlier), kept on whole samples and within the range. */
export function nudgeDelay(ms: number, samples: number, sampleRate: number): number {
  return clamp(((delaySamples(ms, sampleRate) + samples) * 1000) / sampleRate, DELAY_RANGE.min, DELAY_RANGE.max);
}

const signed = (value: number, text: string) => (value > 0 ? `+${text}` : value < 0 ? `−${text}` : text);

export const formatWidth = (width: number): string => `${Math.round(width * 100)}%`;

export const formatDelayMs = (ms: number): string => t().mixer.utility.ms(signed(ms, Math.abs(ms).toFixed(2)));

export function formatDelaySamples(ms: number, sampleRate: number): string {
  const samples = delaySamples(ms, sampleRate);
  return t().mixer.utility.samples(signed(samples, String(Math.abs(samples))));
}
