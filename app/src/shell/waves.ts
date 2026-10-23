/** Each wave: height (fraction of the box), cycles across the width, drift speed (radians per second), opacity, line width. */
const WAVES = [
  { amp: 0.16, cycles: 1.3, speed: 0.35, alpha: 0.24, width: 2 },
  { amp: 0.11, cycles: 2.1, speed: -0.5, alpha: 0.16, width: 1.5 },
  { amp: 0.07, cycles: 3.4, speed: 0.8, alpha: 0.12, width: 1 },
  { amp: 0.2, cycles: 0.8, speed: -0.22, alpha: 0.08, width: 1 },
] as const;
/** A second, faster ripple on top of each wave, relative to it: makes the line look like sound rather than a sine. */
const RIPPLE = { amp: 0.35, cycles: 2.7, speed: 1.6 } as const;
/** Pixels per curve segment: smooth enough and cheap. */
const STEP_PX = 6;

export interface WaveFrame {
  /** The waves' clock, seconds. */
  seconds: number;
  accent: string;
  /** Opacity of the soft radial glow behind the waves; 0 draws none. */
  glowAlpha: number;
}

export function drawWaves(ctx: CanvasRenderingContext2D, w: number, h: number, frame: WaveFrame) {
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
  WAVES.forEach((wave, index) => {
    ctx.globalAlpha = wave.alpha;
    ctx.lineWidth = wave.width;
    ctx.beginPath();
    for (let x = 0; x <= w + STEP_PX; x += STEP_PX) {
      const u = x / w;
      const phase = 2 * Math.PI * wave.cycles * u + frame.seconds * wave.speed + index;
      const wobble = RIPPLE.amp * Math.sin(phase * RIPPLE.cycles - frame.seconds * wave.speed * RIPPLE.speed);
      const y = h / 2 + Math.sin(Math.PI * u) * wave.amp * h * (Math.sin(phase) + wobble);
      if (x === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  });
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1;
}
