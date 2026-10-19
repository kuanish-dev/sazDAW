// Sound-reactive waves in the transport bar, in the free space between the metronome and undo: the welcome screen's
// waveforms (`common/waves.ts`) drift calmly in silence and swell with the master's level — taller, brighter, more
// rippled, trembling and faster as the sound gets louder. The level is the master meter's peak (`session.levels`, from
// the `tick` stream, so nothing new is polled), followed up fast and down slowly so the hits pulse. A frame is four short
// lines on a small canvas; with the system's "reduce motion" setting it draws one still frame.
import { createMemo, onCleanup, onMount } from "solid-js";
import { MASTER_NODE } from "../api/types";
import { follow } from "../effects/liveLevels";
import { dbFraction, toDb } from "../common/plot";
import { useSession } from "../session";
import { fitCanvas } from "../common/canvas";
import { prefersReducedMotion } from "../common/motion";
import { palette } from "../common/theme";
import { drawWaves } from "../common/waves";

/** The level scale, dB: quieter counts as silence. */
const FLOOR_DB = -42;
/** The energy follows the level up at once and down calmly (time constants, s). */
const RISE_SECONDS = 0.02;
const FALL_SECONDS = 0.3;
/** How much faster the waves drift at full level: the tremble and its grain speed up with them. */
const SPEED_BOOST = 2.5;
/** The glow behind the waves at full level (none in silence). */
const LOUD_GLOW = 0.18;
/** Lines brighter than on the welcome screen: the strip is small. */
const BRIGHTNESS = 1.6;
/** The tallest wave, as a share of the strip's height: a loud one stays inside but for its sharpest jags. */
const MAX_AMP = 0.22;
/** Below this the followed level is silence. */
const SILENT_ENERGY = 0.002;
/** Longest frame step, s: after the window was hidden the waves do not jump. */
const MAX_STEP_SECONDS = 0.1;

export function HeaderWaves() {
  const session = useSession();
  let box!: HTMLDivElement;
  let canvas!: HTMLCanvasElement;
  /** Read once per theme or accent change, not on every frame (`palette()` reads computed styles). */
  const accent = createMemo(() => palette().accent);
  let frame = 0;
  let last = 0;
  /** The waves' own clock (it runs faster with the sound) and the followed level, 0..1. */
  let clock = 0;
  let energy = 0;

  const peak = () => {
    const master = session.levels[MASTER_NODE];
    return master ? Math.max(master.l.level, master.r.level) : 0;
  };

  const paint = (dt: number) => {
    energy = follow(energy, dbFraction(toDb(peak()), FLOOR_DB), dt, RISE_SECONDS, FALL_SECONDS);
    // The follower only nears zero: snap it there, so silence draws the calm drift without the tremble.
    if (energy < SILENT_ENERGY) energy = 0;
    clock += dt * (1 + SPEED_BOOST * energy);
    const w = box.clientWidth;
    const h = box.clientHeight;
    if (w > 0 && h > 0) {
      drawWaves(fitCanvas(canvas, w, h), w, h, {
        seconds: clock, accent: accent(), energy, glowAlpha: LOUD_GLOW * energy, brightness: BRIGHTNESS, maxAmp: MAX_AMP,
      });
    }
  };

  onMount(() => {
    if (prefersReducedMotion()) return paint(0);
    const loop = (now: number) => {
      const dt = last === 0 ? 0 : Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
      last = now;
      paint(dt);
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
  });
  onCleanup(() => cancelAnimationFrame(frame));

  return (
    <div ref={box} class="relative h-7 min-w-12 flex-1 overflow-hidden" aria-hidden="true">
      <canvas ref={canvas} class="pointer-events-none absolute left-0 top-0" />
    </div>
  );
}
