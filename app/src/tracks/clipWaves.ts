import { createContext, createSignal, useContext } from "solid-js";
import type { Clip, Id, TempoMap } from "../api/types";
import { bucketsFor, clipColumns, type SamplePeaks } from "./clipWave";
import { averageTempo } from "../common/tempoMap";
import { takeRanges } from "./takes";
import { clipTempo } from "./warp";
import type { SamplesRole } from "../samples/role";

export function createClipWaves(samples: Pick<SamplesRole, "sampleInfo" | "editor">) {
  const [peaks, setPeaks] = createSignal<ReadonlyMap<Id, SamplePeaks>>(new Map());
  const requested = new Set<Id>();
  /** One entry per clip take: a changed clip (resize, tempo) replaces its own entry without evicting others. */
  const columns = new Map<string, { key: string; columns: [number, number][] }>();

  const load = async (sample: Id) => {
    const info = await samples.sampleInfo(sample);
    const wave = info && (await samples.editor.waveform(sample, 0, info.frameCount, bucketsFor(info.frameCount, info.sampleRate)));
    if (!wave) return requested.delete(sample);
    columns.clear();
    setPeaks(new Map(peaks()).set(sample, { frameCount: wave.frameCount, sampleRate: wave.sampleRate, peaks: wave.peaks }));
  };

  /** Waveform columns of the clip's take `take` (position, offset and warping come from the clip); `null` until the envelope arrives. */
  const takeColumns = (clip: Clip, take: Id, width: number, map: TempoMap): [number, number][] | null => {
    const wave = peaks().get(take);
    if (!wave) {
      if (!requested.has(take)) {
        requested.add(take);
        void load(take);
      }
      return null;
    }
    const columnCount = Math.max(1, Math.round(width));
    const soundTempo = clipTempo(clip, averageTempo(map, clip.startTick, clip.startTick + clip.lengthTicks));
    const key = `${columnCount}:${clip.offsetFrames}:${clip.lengthTicks}:${soundTempo}`;
    const slot = `${clip.id}:${take}`;
    const cached = columns.get(slot);
    if (cached?.key === key) return cached.columns;
    const fresh = clipColumns(wave, clip.offsetFrames, clip.lengthTicks, soundTempo, columnCount);
    columns.set(slot, { key, columns: fresh });
    return fresh;
  };

  return {
    takeColumns,
    /** Sample rate of a sound whose envelope has arrived. */
    sampleRateOf: (sample: Id): number | undefined => peaks().get(sample)?.sampleRate,
    /** Waveform columns of the clip `width` pixels wide (for a comp, each segment from its own take); `null` until the envelopes
     *  arrive (requests are sent automatically). */
    columnsFor(clip: Clip, width: number, map: TempoMap): [number, number][] | null {
      if (clip.content.type !== "audio") return null;
      const ranges = takeRanges(clip.content, clip.lengthTicks);
      const parts = ranges.map((range) => takeColumns(clip, range.take, width, map));
      if (parts.some((part) => part === null)) return null;
      if (parts.length === 1) return parts[0]!;
      const count = parts[0]!.length;
      return Array.from({ length: count }, (_, c) => {
        const tick = ((c + 0.5) / count) * clip.lengthTicks;
        const index = Math.max(0, ranges.findIndex((range) => tick < range.to));
        return parts[index]![c]!;
      });
    },
  };
}

export type ClipWaves = ReturnType<typeof createClipWaves>;

export const ClipWavesContext = createContext<ClipWaves>();

/** The Tracks page's waveform cache (inside `TracksView`). */
export function useClipWaves(): ClipWaves {
  const waves = useContext(ClipWavesContext);
  if (!waves) throw new Error("ClipWavesContext is missing");
  return waves;
}
