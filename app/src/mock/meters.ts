// Mock backend meters: plausible "live" node levels while the transport plays, with peak hold, effect gain reduction and
// the band gains of band-split effects.
import type { BandGainsReading, GainReductionReading, MeterReading } from "../api/types";
import { hasStrip } from "../domain/graph";
import type { MockWorld } from "./world";

/** Pseudo-loudness of a playing node: 35 to 70 % of its fader, wobbling. */
const BASE_LEVEL = 0.35;
const SWING_LEVEL = 0.35;
const SWING_PERIOD_MS = 130;
/** How much the held peak falls per tick. */
const PEAK_HOLD = 0.7;

export function mockMeters(w: MockWorld, now: number): MeterReading[] {
  return w.project.graph.nodes.flatMap((node) => {
    const kind = node.kind;
    if (kind.type === "effect") return [];
    const pan = kind.type === "master" ? 0 : kind.pan;
    const playing = w.state === "playing" && !(hasStrip(kind) && kind.mute);
    const audible = playing ? kind.volume * (BASE_LEVEL + SWING_LEVEL * Math.abs(Math.sin(now / SWING_PERIOD_MS + node.id))) : 0;
    const held = Math.max(audible, (w.peaks.get(node.id) ?? 0) * PEAK_HOLD);
    w.peaks.set(node.id, held);
    return [{ node: node.id, left: held * (1 - Math.max(0, pan)), right: held * (1 + Math.min(0, pan)) }];
  });
}

/** Effects that report gain reduction, as in the backend (`Effect::gain_reduction_db`). */
const REDUCING = new Set(["compressor", "gate", "limiter", "multiband"]);
/** Pseudo gain reduction of a playing compressor or limiter, in dB: 0 to 6, wobbling with the meters. */
const SWING_REDUCTION_DB = 6;

/** Plausible gain reduction of non-bypassed reducing effects; a silent gate is closed at its depth, like the real one. */
export function mockReductions(w: MockWorld, now: number): GainReductionReading[] {
  return w.project.graph.nodes.flatMap((node) => {
    const kind = node.kind;
    if (kind.type !== "effect" || kind.bypass || !REDUCING.has(kind.effect)) return [];
    const playing = w.state === "playing";
    if (kind.effect === "gate") return [{ node: node.id, db: playing ? 0 : -(kind.params.range ?? -80) }];
    return [{ node: node.id, db: playing ? SWING_REDUCTION_DB * Math.abs(Math.sin(now / SWING_PERIOD_MS + node.id)) : 0 }];
  });
}

/** Band-split effects and how many bands they report now, as in the backend (`Effect::band_gains_db`). */
const BANDED: Record<string, (params: Record<string, number>) => number> = {
  multiband: (params) => ((params.bands ?? 1) >= 1 ? 4 : 3),
};

/** Plausible band gains of non-bypassed band-split effects: each band wobbles on its own between 0 and the swing. */
export function mockBandGains(w: MockWorld, now: number): BandGainsReading[] {
  return w.project.graph.nodes.flatMap((node) => {
    const kind = node.kind;
    const count = kind.type === "effect" && !kind.bypass ? BANDED[kind.effect]?.(kind.params) : undefined;
    if (count === undefined) return [];
    const playing = w.state === "playing";
    const db = Array.from({ length: count }, (_, band) =>
      playing ? -SWING_REDUCTION_DB * Math.abs(Math.sin(now / (SWING_PERIOD_MS * (band + 1)) + node.id)) : 0);
    return [{ node: node.id, db }];
  });
}
