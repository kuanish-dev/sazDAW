// Sample editor: everything we do with one sound — waveform, slice markers, tracks from regions, saving a region to WAV.
import { WAV_FILTER } from "../api/dialogs";
import type { Id, Waveform } from "../api/types";
import { basename, joinPath, withExtension } from "../domain/format";
import { regionTrack, type RegionTrackSpec } from "../domain/samplerTracks";
import { segments, type Selection } from "../domain/waveform";
import { t } from "../i18n";
import type { Backend, Calls, View } from "./context";
import { addTracks } from "./newTracks";
import { homeDir } from "./paths";

const WAV_EXT = ".wav";

export interface SampleEditorActions {
  waveform(sample: Id, from: number, to: number, buckets: number): Promise<Waveform | undefined>;
  setMarkers(sample: Id, markers: number[]): Promise<void>;
  /** A sampler track from a region of the sample (the created track gets selected). */
  extractSelection(spec: RegionTrackSpec, region: Selection, name: string): Promise<void>;
  /** One track per segment between markers. */
  extractSlices(spec: RegionTrackSpec, markers: number[], baseName: string): Promise<void>;
  /** "Save as WAV" dialog and writing the region. */
  saveRegion(sample: Id, region: Selection, defaultName: string): Promise<void>;
}

export function createSampleEditorActions(ctx: Backend & Calls & View): SampleEditorActions {
  const { api } = ctx;
  return {
    waveform: (sample, from, to, buckets) => ctx.run(api.samples.waveform(sample, from, to, buckets)),
    setMarkers: (sample, markers) => ctx.exec(api.samples.setMarkers(sample, markers)),

    async extractSelection(spec, region, name) {
      if (await addTracks(ctx, [regionTrack(spec, name, region)])) ctx.toast("info", t().samples.trackAdded(name));
    },

    async extractSlices(spec, markers, baseName) {
      const tracks = segments(markers, spec.totalFrames).map((region, i) => regionTrack(spec, `${baseName} ${i + 1}`, region));
      if (await addTracks(ctx, tracks)) ctx.toast("info", t().samples.slicesCreated(tracks.length));
    },

    async saveRegion(sample, region, defaultName) {
      const path = await ctx.dialogs.saveFile({
        title: t().samples.saveRegionTitle,
        filters: [WAV_FILTER],
        defaultPath: joinPath(await homeDir(api), `${defaultName}${WAV_EXT}`),
      });
      if (!path) return;
      const target = withExtension(path, WAV_EXT);
      if (await ctx.attempt(api.samples.exportRegion(sample, region.from, region.to, target))) ctx.toast("info", t().samples.saved(basename(target)));
    },
  };
}
