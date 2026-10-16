// Animated backdrop of the welcome screen: a few slow waveforms drift across the window in the accent color, pinned at
// the edges like a vibrating string. A canvas behind the content, redrawn on every screen frame while the welcome screen
// is shown; with the system's "reduce motion" setting it draws one still frame.
import { createMemo, onCleanup, onMount } from "solid-js";
import { fitCanvas } from "./common/canvas";
import { prefersReducedMotion } from "./common/motion";
import { palette } from "./common/theme";

/** Each wave: height (fraction of the window), cycles across the width, drift speed (radians per second), opacity, line width. */
const WAVES = [
  { amp: 0.16, cycles: 1.3, speed: 0.35, alpha: 0.24, width: 2 },
  { amp: 0.11, cycles: 2.1, speed: -0.5, alpha: 0.16, width: 1.5 },
  { amp: 0.07, cycles: 3.4, speed: 0.8, alpha: 0.12, width: 1 },
  { amp: 0.2, cycles: 0.8, speed: -0.22, alpha: 0.08, width: 1 },
];
/** A second, faster ripple on top of each wave, relative to it: makes the line look like sound rather than a sine. */
const RIPPLE = { amp: 0.35, cycles: 2.7, speed: 1.6 };
/** Pixels per curve segment: smooth enough and cheap. */
const STEP_PX = 6;
/** Opacity of the soft glow behind the content. */
const GLOW_ALPHA = 0.1;

function draw(ctx: CanvasRenderingContext2D, w: number, h: number, seconds: number, accent: string) {
  ctx.clearRect(0, 0, w, h);
  const glow = ctx.createRadialGradient(w / 2, h / 2, 0, w / 2, h / 2, Math.max(w, h) / 2);
  glow.addColorStop(0, accent);
  glow.addColorStop(1, "transparent");
  ctx.globalAlpha = GLOW_ALPHA;
  ctx.fillStyle = glow;
  ctx.fillRect(0, 0, w, h);
  ctx.strokeStyle = accent;
  WAVES.forEach((wave, index) => {
    ctx.globalAlpha = wave.alpha;
    ctx.lineWidth = wave.width;
    ctx.beginPath();
    for (let x = 0; x <= w + STEP_PX; x += STEP_PX) {
      const u = x / w;
      const phase = 2 * Math.PI * wave.cycles * u + seconds * wave.speed + index;
      const ripple = RIPPLE.amp * Math.sin(phase * RIPPLE.cycles - seconds * wave.speed * RIPPLE.speed);
      const y = h / 2 + Math.sin(Math.PI * u) * wave.amp * h * (Math.sin(phase) + ripple);
      if (x === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
  });
  ctx.globalAlpha = 1;
  ctx.lineWidth = 1;
}

export function WelcomeBackdrop() {
  let canvas!: HTMLCanvasElement;
  /** Read once per theme or accent change, not on every frame (`palette()` reads computed styles). */
  const accent = createMemo(() => palette().accent);
  let frame = 0;

  const paint = (now: number) => {
    // The welcome screen's size, not the canvas's own: `fitCanvas` pins the canvas's CSS size, so it would never grow.
    const w = canvas.parentElement?.clientWidth ?? 0;
    const h = canvas.parentElement?.clientHeight ?? 0;
    if (w > 0 && h > 0) draw(fitCanvas(canvas, w, h), w, h, now / 1000, accent());
  };

  onMount(() => {
    if (prefersReducedMotion()) return paint(0);
    const loop = (now: number) => {
      paint(now);
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
  });
  onCleanup(() => cancelAnimationFrame(frame));

  return <canvas ref={canvas} class="pointer-events-none absolute left-0 top-0" aria-hidden="true" />;
}
