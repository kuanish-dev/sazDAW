import type { BandGainsReading, Id } from "../api/types";

/** How fast a shown band gain returns toward 0 dB, dB per second (the gain reduction meter's speed). */
const BAND_FALL_DB = 30;

/** The node's band gains in a tick; empty when it reported none (bypassed, removed). */
export const bandGainsOf = (readings: readonly BandGainsReading[], node: Id): number[] => readings.find((r) => r.node === node)?.db ?? [];

/** The shown gains after `dt` seconds: a deeper cut or a higher boost shows at once, otherwise the shown value returns
 *  toward the fresh one at `BAND_FALL_DB`; no fresh readings let every band return toward 0 dB. */
export function holdBandGains(shown: readonly number[], fresh: readonly number[], dt: number): number[] {
  const target = fresh.length > 0 ? fresh : shown.map(() => 0);
  const step = BAND_FALL_DB * dt;
  return target.map((db, i) => {
    const previous = shown[i] ?? 0;
    if (Math.abs(db) >= Math.abs(previous) || Math.sign(db) * Math.sign(previous) < 0) return db;
    return previous > db ? Math.max(db, previous - step) : Math.min(db, previous + step);
  });
}
