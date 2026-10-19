// The graph of the waveshaping effects — distortion, saturator, tape/console, bitcrusher (`effects/ShaperGraph.tsx`):
// the transfer curve (an input sample → the output sample, both −1..1) and the live sound drawn on it and as waves. The
// curves MIRROR `Shape::apply` and the drive/mix/output path of `crates/daw-effects/src/distortion.rs`, `shapes` with the
// type weights of `saturator.rs`, `soft_knee`/`shape` with the mode characters and drive compensation of `tape.rs`, and
// `quantize` with the mix/output path of `crusher.rs`: a change there must be repeated here. What shapes the sound over
// time rather than sample by sample — the distortion's tone filter, the oversampling and DC blockers, the tape's head
// bump and roll-off, the console's crosstalk, the crusher's downsampling — is left out of the curve; the live trace shows
// it as a loop around the curve.
import type { ParamInfo, Params } from "../api/types";
import { namedParams } from "../common/catalog";
import { clamp } from "../common/math";
import { type PlotView, type Point, linePath } from "../common/plot";

/** An output sample for an input sample. */
export type Transfer = (x: number) => number;

const dbToGain = (db: number) => 10 ** (db / 20);
/** The dry/wet mix and output gain both effects put around their curve. */
const around = (curve: Transfer, drive: number, mix: number, output: number): Transfer => (x) => (x + (curve(x * drive) - x) * mix) * output;

/** The distortion's shapes in the order of its `shape` choice: soft (tanh), hard clip, triangle foldback. */
const DISTORTION_SHAPES: Transfer[] = [
  Math.tanh,
  (x) => clamp(x, -1, 1),
  (x) => {
    // A triangle fold: period 4, range [-1, 1]; loud input folds back instead of flattening.
    const t = (((x + 1) % 4) + 4) % 4;
    return t < 2 ? t - 1 : 3 - t;
  },
];

export function distortionTransfer(infos: ParamInfo[], values: Params): Transfer | null {
  const s = namedParams(infos, values, ["drive", "shape", "mix", "output"]);
  if (!s) return null;
  const shape = DISTORTION_SHAPES[clamp(Math.trunc(s.shape), 0, DISTORTION_SHAPES.length - 1)]!;
  return around(shape, dbToGain(s.drive), s.mix, dbToGain(s.output));
}

/** Offset of the saturator's tube type (`TUBE_BIAS`): the positive half clips earlier than the negative one. */
const TUBE_BIAS = 0.3;
/** The saturator's types in the order of its `type` choice: soft (tanh), tube (asymmetric), hard clip. */
const SATURATOR_TYPES: Transfer[] = [Math.tanh, (x) => Math.tanh(x + TUBE_BIAS) - Math.tanh(TUBE_BIAS), (x) => clamp(x, -1, 1)];

export function saturatorTransfer(infos: ParamInfo[], values: Params): Transfer | null {
  const s = namedParams(infos, values, ["drive", "type", "mix", "output"]);
  if (!s) return null;
  const type = SATURATOR_TYPES[clamp(Math.trunc(s.type), 0, SATURATOR_TYPES.length - 1)]!;
  return around(type, dbToGain(s.drive), s.mix, dbToGain(s.output));
}

/** The tape/console curve (`soft_knee`): linear up to `knee`, then rounding off toward ±1 with no corner. */
function softKnee(x: number, knee: number): number {
  const size = Math.abs(x);
  return size <= knee ? x : Math.sign(x) * (knee + (1 - knee) * Math.tanh((size - knee) / (1 - knee)));
}

/** The modes' knee and offset in the order of the `mode` choice: tape, console (`TAPE`, `CONSOLE`). */
const TAPE_MODES = [{ knee: 0.4, bias: 0.08 }, { knee: 0.75, bias: 0.02 }];

export function tapeTransfer(infos: ParamInfo[], values: Params): Transfer | null {
  const s = namedParams(infos, values, ["mode", "drive", "output"]);
  if (!s) return null;
  const { knee, bias } = TAPE_MODES[clamp(Math.trunc(s.mode), 0, TAPE_MODES.length - 1)]!;
  const drive = dbToGain(s.drive);
  const output = dbToGain(s.output);
  // Driven into the knee and divided by the drive again: quiet samples keep their level, the peaks round off.
  return (x) => ((softKnee(x * drive + bias, knee) - softKnee(bias, knee)) / drive) * output;
}

/** The bitcrusher's staircase: every sample rounded to `2^(bits−1)` steps each side of zero, mixed with the clean one. */
export function crusherTransfer(infos: ParamInfo[], values: Params): Transfer | null {
  const s = namedParams(infos, values, ["bits", "mix", "output"]);
  if (!s) return null;
  const steps = 2 ** (s.bits - 1);
  const output = dbToGain(s.output);
  return (x) => (x * (1 - s.mix) + (Math.round(x * steps) / steps) * s.mix) * output;
}

/** A sample (−1..1) as a position in a view: x across the width, y up from the bottom; louder is clipped to the edge. */
const sampleX = (view: PlotView, x: number) => ((clamp(x, -1, 1) + 1) / 2) * view.width;
const sampleY = (view: PlotView, y: number) => ((1 - clamp(y, -1, 1)) / 2) * view.height;

/** The SVG path of the curve over input −1..1. */
export function transferPath(transfer: Transfer, view: PlotView, points = 160): string {
  const line: Point[] = [];
  for (let i = 0; i <= points; i++) {
    const x = -1 + (2 * i) / points;
    line.push([sampleX(view, x), sampleY(view, transfer(x))]);
  }
  return linePath(line);
}

/** How many of the newest frames the live trace and the waves show: about 21 ms at 48 kHz, a period of a low bass note. */
export const TRACE_FRAMES = 1024;

/** The live sound on the curve: each frame's input sample against its output sample (an X-Y scope) over the newest
 *  `TRACE_FRAMES`; a pure waveshaper draws its curve, a filter in the effect opens it into a loop. */
export function tracePath(input: Float32Array, output: Float32Array, view: PlotView): string {
  const count = Math.min(input.length, output.length, TRACE_FRAMES);
  const from = Math.min(input.length, output.length) - count;
  const line: Point[] = [];
  for (let i = from; i < from + count; i++) line.push([sampleX(view, input[i]!), sampleY(view, output[i]!)]);
  return linePath(line);
}

/** One wave over `span` samples from `start`, across the whole view width (−1 at the bottom, 1 at the top). */
export function wavePath(samples: Float32Array, start: number, span: number, view: PlotView): string {
  const line: Point[] = [];
  const end = Math.min(samples.length, start + span);
  for (let i = start; i < end; i++) line.push([((i - start) / Math.max(1, span - 1)) * view.width, sampleY(view, samples[i]!)]);
  return linePath(line);
}
