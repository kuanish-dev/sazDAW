// Animated backdrop of the welcome screen: a few slow waveforms drift across the window in the accent color, pinned at
// the edges like a vibrating string (`shell/waves.ts`). A canvas behind the content,
// redrawn on every screen frame while the welcome screen is shown; with the system's "reduce motion" setting it draws
// one still frame.
import { createMemo, onCleanup, onMount } from "solid-js";
import { fitCanvas } from "../common/canvas";
import { prefersReducedMotion } from "../common/motion";
import { palette } from "../common/theme";
import { drawWaves } from "./waves";

/** Opacity of the soft glow behind the content. */
const GLOW_ALPHA = 0.1;

export function WelcomeBackdrop() {
  let canvas!: HTMLCanvasElement;
  /** Read once per theme or accent change, not on every frame (`palette()` reads computed styles). */
  const accent = createMemo(() => palette().accent);
  let frame = 0;

  const paint = (now: number) => {
    // The welcome screen's size, not the canvas's own: `fitCanvas` pins the canvas's CSS size, so it would never grow.
    const w = canvas.parentElement?.clientWidth ?? 0;
    const h = canvas.parentElement?.clientHeight ?? 0;
    if (w > 0 && h > 0) drawWaves(fitCanvas(canvas, w, h), w, h, { seconds: now / 1000, accent: accent(), glowAlpha: GLOW_ALPHA });
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
