import { PPQ, type SignatureChange, type TempoChange, type TempoMap, type TimeSignature } from "../api/types";
import { BAR_GRID, barTicks, snapDown, type BarPosition } from "./time";

/** Map limits, as in the backend (the mock checks them). */
export const MAX_TEMPO_CHANGES = 512;
export const MAX_SIGNATURE_CHANGES = 256;
export const MAX_TICK = PPQ * 60 * 500;

export const constantMap = (bpm: number, signature: TimeSignature): TempoMap => ({
  tempos: [{ tick: 0, bpm, ramp: false }],
  signatures: [{ tick: 0, signature }],
});

const secondsPerTick = (bpm: number) => 60 / (PPQ * bpm);

/** The last item with `tick` not after `tick` (before the song start, the first). */
function indexAt(items: readonly { tick: number }[], tick: number): number {
  let index = 0;
  for (let i = 1; i < items.length && items[i]!.tick <= tick; i++) index = i;
  return index;
}

/** If a ramp follows point `i`: its end tempo and length in ticks. */
function rampAfter(tempos: readonly TempoChange[], i: number): { to: number; span: number } | null {
  const next = tempos[i + 1];
  return next?.ramp ? { to: next.bpm, span: next.tick - tempos[i]!.tick } : null;
}

/** Seconds for `ticks` ticks from point `i` (not past the next point). */
function segmentSeconds(tempos: readonly TempoChange[], i: number, ticks: number): number {
  const from = tempos[i]!.bpm;
  const ramp = rampAfter(tempos, i);
  if (!ramp || ramp.to === from) return ticks * secondsPerTick(from);
  const slope = (ramp.to - from) / ramp.span;
  return (60 / PPQ) * Math.log((from + slope * ticks) / from) / slope;
}

/** Seconds from the song start to each tempo point. */
function pointSeconds(tempos: readonly TempoChange[]): number[] {
  const seconds = [0];
  for (let i = 1; i < tempos.length; i++) seconds.push(seconds[i - 1]! + segmentSeconds(tempos, i - 1, tempos[i]!.tick - tempos[i - 1]!.tick));
  return seconds;
}

/** Tempo at `tick` (inside a ramp, the intermediate value). */
export function tempoAt(map: TempoMap, tick: number): number {
  const i = indexAt(map.tempos, tick);
  const from = map.tempos[i]!;
  const ramp = rampAfter(map.tempos, i);
  if (!ramp) return from.bpm;
  const share = Math.min(1, Math.max(0, (tick - from.tick) / ramp.span));
  return from.bpm + (ramp.to - from.bpm) * share;
}

export const signatureAt = (map: TempoMap, tick: number): SignatureChange => map.signatures[indexAt(map.signatures, tick)]!;

/** Seconds from the song start to `tick` (before the start, at the tempo of tick 0). */
export function secondsAt(map: TempoMap, tick: number): number {
  const base = map.tempos[0]!.bpm;
  if (tick <= 0) return tick * secondsPerTick(base);
  const i = indexAt(map.tempos, tick);
  return pointSeconds(map.tempos)[i]! + segmentSeconds(map.tempos, i, tick - map.tempos[i]!.tick);
}

/** The tick reached `seconds` seconds after the song start. */
export function tickAtSeconds(map: TempoMap, seconds: number): number {
  const base = map.tempos[0]!.bpm;
  if (seconds <= 0) return seconds / secondsPerTick(base);
  const starts = pointSeconds(map.tempos);
  const i = indexAt(starts.map((s) => ({ tick: s })), seconds);
  const from = map.tempos[i]!;
  const elapsed = seconds - starts[i]!;
  const ramp = rampAfter(map.tempos, i);
  if (!ramp || ramp.to === from.bpm) return from.tick + elapsed / secondsPerTick(from.bpm);
  const slope = (ramp.to - from.bpm) / ramp.span;
  const bpm = from.bpm * Math.exp((elapsed * slope * PPQ) / 60);
  return from.tick + (bpm - from.bpm) / slope;
}

export const framesBetween = (map: TempoMap, from: number, to: number, sampleRate: number): number =>
  (secondsAt(map, to) - secondsAt(map, from)) * sampleRate;

export const tickAfterFrames = (map: TempoMap, from: number, frames: number, sampleRate: number): number =>
  tickAtSeconds(map, secondsAt(map, from) + frames / sampleRate);

/** Average tempo over ticks `[from, to)`: beats per seconds (for a clip waveform on the timeline). */
export function averageTempo(map: TempoMap, from: number, to: number): number {
  const seconds = secondsAt(map, to) - secondsAt(map, from);
  return seconds > 0 ? ((to - from) / PPQ) * (60 / seconds) : tempoAt(map, from);
}

/** Start of the bar containing `tick`. */
export function barStart(map: TempoMap, tick: number): number {
  const change = signatureAt(map, tick);
  const bar = barTicks(change.signature);
  return change.tick + Math.floor((tick - change.tick) / bar) * bar;
}

/** Length in ticks of the bar containing `tick`. */
export const barLengthAt = (map: TempoMap, tick: number): number => barTicks(signatureAt(map, tick).signature);

/** Fractional bars from the song start to `tick`: bar index plus the part of it already passed. */
const barsAt = (map: TempoMap, tick: number): number => barIndex(map, tick) + (tick - barStart(map, tick)) / barLengthAt(map, tick);

/** Fractional bars between ticks `from` and `to` by time signature changes. */
export const barsBetween = (map: TempoMap, from: number, to: number): number => barsAt(map, to) - barsAt(map, from);

/**
 * Grid points on both sides of `tick`. `BAR_GRID` means the map's bar boundaries; another step counts from the bar
 * start, and the bar end is a point too, so an odd-meter bar doesn't shift the grid of the following bars.
 */
function gridAround(map: TempoMap, tick: number, grid: number): [number, number] {
  const start = barStart(map, tick);
  const end = start + barLengthAt(map, tick);
  if (grid === BAR_GRID) return [start, end];
  const down = start + snapDown(tick - start, grid);
  return [down, Math.min(down + grid, end)];
}

/** Snap down to grid `grid` within the map's bars. */
export const snapDownInBars = (map: TempoMap, tick: number, grid: number): number => gridAround(map, tick, grid)[0];

/** Snap to the nearest point of grid `grid` within the map's bars (exactly halfway goes forward, like `Math.round`). */
export function snapNearestInBars(map: TempoMap, tick: number, grid: number): number {
  const [down, up] = gridAround(map, tick, grid);
  return tick - down < up - tick ? down : up;
}

/** Zero-based index of the bar containing `tick`. */
export function barIndex(map: TempoMap, tick: number): number {
  const j = indexAt(map.signatures, tick);
  let bars = 0;
  for (let k = 0; k < j; k++) bars += (map.signatures[k + 1]!.tick - map.signatures[k]!.tick) / barTicks(map.signatures[k]!.signature);
  const change = map.signatures[j]!;
  return bars + Math.floor((tick - change.tick) / barTicks(change.signature));
}

/** Start tick of zero-based bar `bar`. */
export function barTick(map: TempoMap, bar: number): number {
  let first = 0;
  for (let j = 0; j < map.signatures.length; j++) {
    const change = map.signatures[j]!;
    const length = barTicks(change.signature);
    const next = map.signatures[j + 1];
    const bars = next ? (next.tick - change.tick) / length : Infinity;
    if (bar < first + bars) return change.tick + (bar - first) * length;
    first += bars;
  }
  return 0;
}

/** Absolute ticks → bar:beat:tick (one-based) by time signature changes. */
export function barPosition(map: TempoMap, tick: number): BarPosition {
  const { signature } = signatureAt(map, tick);
  const start = barStart(map, tick);
  const perBeat = (PPQ * 4) / signature.denominator;
  const beat = Math.floor((tick - start) / perBeat);
  return { bar: barIndex(map, tick) + 1, beat: beat + 1, tick: Math.floor(tick - start - beat * perBeat) };
}

/** A grid line at a beat; `bar` is the one-based bar number if the beat starts a bar. */
export interface GridLine {
  tick: number;
  bar: number | null;
}

/** Beats and bars in `[from, to]` by time signature changes (bars of different lengths). */
export function gridLines(map: TempoMap, from: number, to: number): GridLine[] {
  const lines: GridLine[] = [];
  let bar = barIndex(map, Math.max(0, from));
  let start = barTick(map, bar);
  while (start <= to) {
    const { signature } = signatureAt(map, start);
    const perBeat = (PPQ * 4) / signature.denominator;
    for (let beat = 0; beat < signature.numerator; beat++) {
      const tick = start + beat * perBeat;
      if (tick >= from && tick <= to) lines.push({ tick, bar: beat === 0 ? bar + 1 : null });
    }
    bar += 1;
    start = barTick(map, bar);
  }
  return lines;
}

/** The tempo of the point in effect at `tick` (the last one not after it) becomes `bpm`. */
export function withTempoAt(map: TempoMap, tick: number, bpm: number): TempoChange[] {
  const i = indexAt(map.tempos, tick);
  return map.tempos.map((change, k) => (k === i ? { ...change, bpm } : change));
}

/**
 * Time signature changes by bar number → by tick (bars of different lengths are counted in order). This way an earlier
 * signature edit doesn't put later changes mid-bar: they stay on their bar numbers.
 */
function placeByBars(byBar: { bar: number; signature: TimeSignature }[]): SignatureChange[] {
  const sorted = [...byBar].sort((a, b) => a.bar - b.bar);
  const placed: SignatureChange[] = [];
  for (const { bar, signature } of sorted) {
    const last = placed.at(-1);
    const lastBar = last ? sorted[placed.length - 1]!.bar : 0;
    const tick = last ? last.tick + (bar - lastBar) * barTicks(last.signature) : 0;
    placed.push({ tick, signature });
  }
  return placed;
}

const signaturesByBar = (map: TempoMap) => map.signatures.map((c) => ({ bar: barIndex(map, c.tick), signature: c.signature }));

/** Time signature from the bar containing `tick` (the change goes to its start); later changes stay on their bars. */
export function withSignatureFrom(map: TempoMap, tick: number, signature: TimeSignature): SignatureChange[] {
  const bar = barIndex(map, Math.max(0, tick));
  const others = signaturesByBar(map).filter((c) => c.bar !== bar);
  return placeByBars([...others, { bar, signature }]);
}

/** The time signature in effect at `tick` becomes `signature` (edits that change rather than adding one). */
export function withSignatureAt(map: TempoMap, tick: number, signature: TimeSignature): SignatureChange[] {
  return withSignatureFrom(map, signatureAt(map, tick).tick, signature);
}

/** Without time signature change `index` (the first can't be removed); later changes stay on their bars. */
export function withoutSignature(map: TempoMap, index: number): SignatureChange[] {
  if (index === 0) return map.signatures;
  return placeByBars(signaturesByBar(map).filter((_, k) => k !== index));
}
