// The biquad filters of the built-in effects for their graphs: the coefficients by the RBJ Audio EQ Cookbook and the
// response at a frequency. MIRRORS `crates/daw-effects/src/biquad.rs`: a change there must be repeated here.

/** Sample rate for drawing: the response near the top of the band differs slightly at other rates. */
export const DRAW_RATE = 48_000;
/** Butterworth Q: the flattest passband (`BUTTERWORTH_Q`), also the Q of a Linkwitz–Riley split's halves and all-pass. */
export const BUTTERWORTH_Q = Math.SQRT1_2;

export interface Coefficients {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

export const PASS: Readonly<Coefficients> = { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 } as const;

function normalized(b0: number, b1: number, b2: number, a0: number, a1: number, a2: number): Coefficients {
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

/** `side` = 1 low shelf, −1 high shelf (as `Coefficients::shelf`). */
export function shelf(freq: number, gainDb: number, side: number): Coefficients {
  const a = 10 ** (gainDb / 40);
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const cos = side * Math.cos(w);
  const beta = 2 * Math.sqrt(a) * ((Math.sin(w) / 2) * Math.SQRT2);
  return normalized(
    a * (a + 1 - (a - 1) * cos + beta),
    side * 2 * a * (a - 1 - (a + 1) * cos),
    a * (a + 1 - (a - 1) * cos - beta),
    a + 1 + (a - 1) * cos + beta,
    side * -2 * (a - 1 + (a + 1) * cos),
    a + 1 + (a - 1) * cos - beta,
  );
}

export function peaking(freq: number, gainDb: number, q: number): Coefficients {
  const a = 10 ** (gainDb / 40);
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const alpha = Math.sin(w) / (2 * q);
  const cos = Math.cos(w);
  return normalized(1 + alpha * a, -2 * cos, 1 - alpha * a, 1 + alpha / a, -2 * cos, 1 - alpha / a);
}

/** The angle and alpha of the filters whose frequency is clamped at 0.45 of the rate (the passes and the all-pass). */
function clampedAngle(freq: number, q: number): { cos: number; alpha: number } {
  const w = (2 * Math.PI * Math.min(freq, DRAW_RATE * 0.45)) / DRAW_RATE;
  return { cos: Math.cos(w), alpha: Math.sin(w) / (2 * q) };
}

/** `high` = true high-pass, false low-pass. */
export function pass(freq: number, q: number, high: boolean): Coefficients {
  const { cos, alpha } = clampedAngle(freq, q);
  const edge = high ? (1 + cos) / 2 : (1 - cos) / 2;
  return normalized(edge, high ? -(1 + cos) : 1 - cos, edge, 1 + alpha, -2 * cos, 1 - alpha);
}

/** Band-pass with 0 dB at its centre; with the notch of the same frequency and Q it sums to the input. */
export function bandPass(freq: number, q: number): Coefficients {
  const { cos, alpha } = clampedAngle(freq, q);
  return normalized(alpha, 0, -alpha, 1 + alpha, -2 * cos, 1 - alpha);
}

/** All-pass: flat magnitude, the phase a Linkwitz–Riley split puts on its summed sides (at Butterworth Q). */
export function allPass(freq: number, q: number): Coefficients {
  const { cos, alpha } = clampedAngle(freq, q);
  return normalized(1 - alpha, -2 * cos, 1 + alpha, 1 + alpha, -2 * cos, 1 - alpha);
}

/** A complex number as [re, im]. */
export type Complex = readonly [number, number];

const times = ([ar, ai]: Complex, [br, bi]: Complex): Complex => [ar * br - ai * bi, ar * bi + ai * br];

/** The complex response of one biquad at `freq` (for filters whose outputs are summed: the phases matter). */
export function response(c: Coefficients, freq: number): Complex {
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const [cos1, sin1, cos2, sin2] = [Math.cos(w), Math.sin(w), Math.cos(2 * w), Math.sin(2 * w)];
  const [nr, ni] = [c.b0 + c.b1 * cos1 + c.b2 * cos2, -(c.b1 * sin1 + c.b2 * sin2)];
  const [dr, di] = [1 + c.a1 * cos1 + c.a2 * cos2, -(c.a1 * sin1 + c.a2 * sin2)];
  const den = Math.max(dr * dr + di * di, 1e-20);
  return [(nr * dr + ni * di) / den, (ni * dr - nr * di) / den];
}

/** The response of filters in a row at `freq`. */
export function chainResponse(chain: readonly Coefficients[], freq: number): Complex {
  return chain.reduce<Complex>((total, c) => times(total, response(c, freq)), [1, 0]);
}

/** Response of one biquad at `freq`, in dB. */
export function magnitudeDb(c: Coefficients, freq: number): number {
  const w = (2 * Math.PI * freq) / DRAW_RATE;
  const [cos1, sin1, cos2, sin2] = [Math.cos(w), Math.sin(w), Math.cos(2 * w), Math.sin(2 * w)];
  const num = (c.b0 + c.b1 * cos1 + c.b2 * cos2) ** 2 + (c.b1 * sin1 + c.b2 * sin2) ** 2;
  const den = (1 + c.a1 * cos1 + c.a2 * cos2) ** 2 + (c.a1 * sin1 + c.a2 * sin2) ** 2;
  return 10 * Math.log10(Math.max(num, 1e-20) / Math.max(den, 1e-20));
}

/** A complex response as a level in dB. */
export const complexDb = ([re, im]: Complex) => 10 * Math.log10(Math.max(re * re + im * im, 1e-20));
