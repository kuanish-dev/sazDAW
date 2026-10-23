import { createMemo, onMount } from "solid-js";
import type { MasterWindow } from "../api/types";
import { decayPeak } from "./mixer";
import { magnitudes, spectrumBands, traceStart } from "./visualizer";
import { createWatchedPoll } from "../core/poller";
import { fitCanvas } from "../common/canvas";
import { palette } from "../common/theme";
import { useMixer } from "./role";

const WIDTH = 128;
const HEIGHT = 64;
const POLL_MS = 33;
const BAND_COUNT = 32;
const BAND_GAP_PX = 1;
/** How fast a band falls when the sound in it stops, in its 0..1 height per second: smooth, but still lively. */
const BAND_FALL = 1.4;
/** Samples the oscilloscope shows: about 23 ms at 44.1 kHz, one period of a low bass note. */
const TRACE_SAMPLES = 1024;
/** Longest fall step, seconds: after a pause in polling the bars do not drop to zero in one jump. */
const MAX_STEP_SECONDS = 0.2;

export function MasterVisualizer() {
  const { master } = useMixer();
  let canvas!: HTMLCanvasElement;
  /** The theme colors, read once per theme change rather than on every frame. */
  const colors = createMemo(() => palette());
  let bands: number[] = [];
  let last = performance.now();

  const draw = (latest: MasterWindow) => {
    const now = performance.now();
    const dt = Math.min(MAX_STEP_SECONDS, (now - last) / 1000);
    last = now;
    const fresh = latest.samples.length > 0 ? spectrumBands(magnitudes(latest.samples), latest.sampleRate, BAND_COUNT) : [];
    bands = Array.from({ length: BAND_COUNT }, (_, i) => decayPeak(bands[i] ?? 0, fresh[i] ?? 0, dt, BAND_FALL));

    const P = colors();
    const ctx = fitCanvas(canvas, WIDTH, HEIGHT);
    ctx.fillStyle = P.bgDeep;
    ctx.fillRect(0, 0, WIDTH, HEIGHT);
    const fill = ctx.createLinearGradient(0, HEIGHT, 0, 0);
    fill.addColorStop(0, P.accent);
    fill.addColorStop(1, P.noteSelected);
    ctx.fillStyle = fill;
    const barWidth = WIDTH / BAND_COUNT;
    bands.forEach((height, i) => {
      const h = Math.round(height * HEIGHT);
      if (h > 0) ctx.fillRect(i * barWidth, HEIGHT - h, barWidth - BAND_GAP_PX, h);
    });

    const samples = latest.samples;
    if (samples.length === 0) return;
    const span = Math.min(TRACE_SAMPLES, samples.length);
    const start = traceStart(samples, span);
    const step = span / (WIDTH * 2);
    ctx.strokeStyle = P.ink;
    ctx.globalAlpha = 0.85;
    ctx.lineWidth = 1.25;
    ctx.beginPath();
    for (let i = 0; i < span; i += step) {
      const x = (i / span) * WIDTH;
      const y = HEIGHT / 2 - (samples[start + Math.floor(i)] ?? 0) * (HEIGHT / 2) * 0.9;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.lineWidth = 1;
  };

  const capture = createWatchedPoll({ read: master.masterWindow, show: draw }, POLL_MS);
  onMount(() => {
    draw({ samples: new Float32Array(0), sampleRate: 0 });
    capture.start();
  });

  return <canvas ref={canvas} class="block rounded-sm" style={{ width: `${WIDTH}px`, height: `${HEIGHT}px` }} aria-hidden="true" />;
}
