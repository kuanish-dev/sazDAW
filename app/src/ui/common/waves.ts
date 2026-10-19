// Drifting waveforms in the accent color, pinned at the edges like a vibrating string: the welcome screen's backdrop
// (`ui/WelcomeBackdrop.tsx`) and the transport bar's sound-reactive strip (`ui/transport/HeaderWaves.tsx`). `energy`
// (0..1) is how loud the sound is: 0 draws the calm drift, louder makes the waves taller, brighter, more rippled and
// nervous — a fast tremble and a grain that changes many times a second, both absent in silence.
import { clamp } from "../../domain/math";

/** Each wave: height (fraction of the box), cycles across the width, drift speed (radians per second), opacity, line width. */
const WAVES = [
  { amp: 0.16, cycles: 1.3, speed: 0.35, alpha: 0.24, width: 2 },
  { amp: 0.11, cycles: 2.1, speed: -0.5, alpha: 0.16, width: 1.5 },
  { amp: 0.07, cycles: 3.4, speed: 0.8, alpha: 0.12, width: 1 },
  { amp: 0.2, cycles: 0.8, speed: -0.22, alpha: 0.08, width: 1 },
];
/** A second, faster ripple on top of each wave, relative to it: makes the line look like sound rather than a sine. */
const RIPPLE = { amp: 0.35, cycles: 2.7, speed: 1.6 };
/** What full `energy` adds: the height grows by this share, the ripple, the opacity and the tremble by these amounts. */
const LOUD = { amp: 1.6, ripple: 0.6, alpha: 0.35, tremble: 0.55 };
/** The nervous part, only with sound: a dense tremble running fast (each wave its own way), and a grain that jumps
 *  `steps` times per second of the waves' clock, `grain` of the tremble's size. */
const TREMBLE = { cycles: 11, speed: 9, grain: 0.45, steps: 30 };
/** Pixels per curve segment: smooth enough and cheap. */
const STEP_PX = 6;

/** A fixed jagged value in −1..1 for a point at a moment (a hash, no state): the same inputs give the same jag. */
function grain(x: number, moment: number, wave: number): number {
  const n = Math.sin(x * 12.9898 + moment * 78.233 + wave * 37.719) * 43_758.5453;
  return 2 * (n - Math.floor(n)) - 1;
}

export interface WaveFrame {
  /** The waves' clock, seconds: a caller may run it faster than real time. */
  seconds: number;
  accent: string;
  /** How loud the sound is, 0..1. */
  energy: number;
  /** Opacity of the soft radial glow behind the waves; 0 draws none. */
  glowAlpha: number;
  /** Multiplies every wave's opacity: a small strip needs brighter lines than a whole window. */
  brightness?: number;
  /** The tallest a wave may get, as a fraction of the height: keeps a loud wave inside a thin strip. */
  maxAmp?: number;
}

export function drawWaves(ctx: CanvasRenderingContext2D, w: number, h: number, frame: WaveFrame) {
  const energy = clamp(frame.energy, 0, 1);
  ctx.clearRect(0, 0, w, h);
  if (frame.glowAlpha > 0) {
    const glow = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, Math.max(w, h) / 2);
    glow.addColorStop(0, frame.accent);
    glow.addColorStop(1, "transparent");
    ctx.globalAlpha = frame.glowAlpha;
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, w, h);
  }
  ctx.strokeStyle = frame.accent;
  const ripple = RIPPLE.amp + LOUD.ripple * energy;
  const tremble = LOUD.tremble * energy;
  const moment = Math.floor(frame.seconds * TREMBLE.steps);
  WAVES.forEach((wave, index) => {
    ctx.globalAlpha = Math.min(1, (wave.alpha + LOUD.alpha * energy) * (frame.brightness ?? 1));
    ctx.lineWidth = wave.width;
    const amp = Math.min(wave.amp * (1 + LOUD.amp * energy), frame.maxAmp ?? Number.POSITIVE_INFINITY);
    ctx.beginPath();
    for (let x = 0; x <= w + STEP_PX; x += STEP_PX) {
      const u = x / w;
      const phase = 2 * Math.PI * wave.cycles * u + frame.seconds * wave.speed + index;
      const wobble = ripple * Math.sin(phase * RIPPLE.cycles - frame.seconds * wave.speed * RIPPLE.speed);
      // Skipped in silence: the calm drift costs no more than before.
      const nervous =
        tremble > 0
          ? tremble *
            (Math.sin(2 * Math.PI * TREMBLE.cycles * (1 + index * 0.37) * u + frame.seconds * TREMBLE.speed * (index % 2 === 0 ? 1 : -1)) +
              TREMBLE.grain * grain(x, moment, index))
          : 0;
      const y = h / 2 + Math.sin(Math.PI * u) * amp * h * (Math.sin(phase) + wobble + nervous);
      if (x === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  });
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1;
}
