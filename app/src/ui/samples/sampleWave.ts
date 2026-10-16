// Waveform of the selected sample for the editor: file info, view window, envelope of the visible window and an overview of the whole file.
// The window waveform is requested with a short delay (wheel zoom fires dozens of events); stale replies are dropped.
import { createEffect, createSignal, on, onCleanup } from "solid-js";
import type { Id, Waveform } from "../../api/types";
import { clampView, fitAll, type View } from "../../domain/waveform";
import { useSession } from "../../state/session";

/** Number of buckets in the whole-file overview strip. */
const OVERVIEW_BUCKETS = 600;
/** Delay before requesting the window waveform after the last view change, ms. */
const WAVE_DEBOUNCE_MS = 30;
/** Sample rate used until file info arrives (labels only). */
const FALLBACK_RATE = 48_000;

export function createSampleWave(deps: { sample: () => Id | undefined; width: () => number }) {
  const { samples, sampleEditor } = useSession().actions;
  const [meta, setMeta] = createSignal<{ frameCount: number; sampleRate: number } | null>(null);
  const [view, setView] = createSignal<View>({ start: 0, span: 1 });
  const [wave, setWave] = createSignal<Waveform | null>(null);
  const [overview, setOverview] = createSignal<Waveform | null>(null);

  const total = () => meta()?.frameCount ?? 0;

  // Sample changed: reset the window and load the size and overview.
  createEffect(on(deps.sample, async (id) => {
    setWave(null);
    setOverview(null);
    if (id === undefined) return setMeta(null);
    const info = await samples.sampleInfo(id);
    if (!info || deps.sample() !== id) return;
    setMeta({ frameCount: info.frameCount, sampleRate: info.sampleRate });
    setView(fitAll(info.frameCount));
    const full = await sampleEditor.waveform(id, 0, info.frameCount, OVERVIEW_BUCKETS);
    if (full && deps.sample() === id) setOverview(full);
  }));

  let requestToken = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const id = deps.sample();
    const v = view();
    const w = deps.width();
    if (id === undefined || total() === 0) return;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const token = ++requestToken;
      const result = await sampleEditor.waveform(id, Math.floor(v.start), Math.ceil(v.start + v.span), w);
      if (result && token === requestToken) setWave(result);
    }, WAVE_DEBOUNCE_MS);
  });
  onCleanup(() => clearTimeout(timer));

  return {
    meta,
    total,
    rate: () => meta()?.sampleRate ?? FALLBACK_RATE,
    view,
    /** Sets a new view window, clamped to the file. */
    updateView: (next: View) => setView(clampView(next, total())),
    fitAll: () => setView(fitAll(total())),
    wave,
    overview,
  };
}
