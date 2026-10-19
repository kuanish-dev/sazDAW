// Volume and pan: dB, meter scale, fader curve, meter ballistics (peak fall-off, peak hold, clip light).
import { t } from "../i18n";
import { clamp } from "../common/math";

export const MIN_DB = -60;
export const MAX_DB = 6;

/** Linear volume (1.0 = 0 dB) → dB. */
export function volumeToDb(volume: number): number {
  return volume <= 0 ? -Infinity : 20 * Math.log10(volume);
}

export function formatDb(volume: number): string {
  const db = volumeToDb(volume);
  return Number.isFinite(db) ? t().mixer.db(`${db >= 0 ? "+" : ""}${db.toFixed(1)}`) : "−∞";
}

/** Linear peak → position on the meter scale 0..1. */
export function peakToMeter(peak: number): number {
  const db = volumeToDb(peak);
  if (!Number.isFinite(db) || db <= MIN_DB) return 0;
  return Math.min(1, (db - MIN_DB) / (MAX_DB - MIN_DB));
}

/** Volume at the top fader position (as the engine's `MAX_LEVEL`). */
const FADER_MAX_VOLUME = 1.5;

/** Fader position 0..1 ↔ volume 0..1.5, weighted to the working range (^2 curve). */
export function faderToVolume(position: number): number {
  return Math.min(FADER_MAX_VOLUME, Math.max(0, position) ** 2 * FADER_MAX_VOLUME);
}

export function volumeToFader(volume: number): number {
  return Math.sqrt(clamp(volume, 0, FADER_MAX_VOLUME) / FADER_MAX_VOLUME);
}

/** Fader step for the wheel and arrows, fraction of travel. */
export const FADER_STEP = 0.02;

export const nudgeFader = (volume: number, delta: number): number => faderToVolume(clamp(volumeToFader(volume) + delta, 0, 1));

export function formatPan(pan: number): string {
  if (Math.abs(pan) < 0.005) return "C";
  return `${Math.round(Math.abs(pan) * 100)}${pan < 0 ? "L" : "R"}`;
}

/** Meter peak hold: fast up, smooth down. */
export function decayPeak(previous: number, current: number, dt: number, fallPerSecond = 1.5): number {
  return Math.max(current, previous - fallPerSecond * dt);
}

/** One meter channel as shown: the falling bar, the peak-hold tick and how long it has held, and how long the clip light
 *  stays lit (seconds; `0` — off). Linear peaks, like the engine's. */
export interface MeterChannel {
  level: number;
  hold: number;
  holdAge: number;
  clip: number;
}

/** A node's meter: left and right channels. */
export interface MeterLevel {
  l: MeterChannel;
  r: MeterChannel;
}

export const SILENT_CHANNEL: MeterChannel = { level: 0, hold: 0, holdAge: 0, clip: 0 };
export const SILENT_LEVEL: MeterLevel = { l: SILENT_CHANNEL, r: SILENT_CHANNEL };

/** The peak-hold tick stays put this long (seconds) before it falls. */
const HOLD_SECONDS = 1.2;
/** How fast a released peak-hold tick falls, linear peak per second: slower than the bar, so it trails behind it. */
const HOLD_FALL = 0.6;
/** A peak at or above full scale (0 dBFS) keeps the clip light on this long, seconds. */
const CLIP_SECONDS = 2;

/** One meter step: `peak` is the channel's linear peak since the previous step, `dt` the seconds since it. */
export function stepMeter(prev: MeterChannel, peak: number, dt: number): MeterChannel {
  const level = decayPeak(prev.level, peak, dt);
  const fresh = peak >= prev.hold;
  const holdAge = fresh ? 0 : prev.holdAge + dt;
  let hold = peak;
  if (!fresh) hold = holdAge < HOLD_SECONDS ? prev.hold : decayPeak(prev.hold, level, dt, HOLD_FALL);
  return { level, hold, holdAge, clip: peak >= 1 ? CLIP_SECONDS : Math.max(0, prev.clip - dt) };
}

/** A meter from readings that already fall at their source (the input level): no hold tick, the clip light only while at
 *  or above full scale. */
export function plainLevel(left: number, right: number): MeterLevel {
  const channel = (peak: number): MeterChannel => ({ level: peak, hold: 0, holdAge: 0, clip: peak >= 1 ? CLIP_SECONDS : 0 });
  return { l: channel(left), r: channel(right) };
}

/** Nothing left to show: the bar, the hold tick and the clip light are all down. */
export const isSilent = (channel: MeterChannel): boolean => channel.level === 0 && channel.hold === 0 && channel.clip === 0;
