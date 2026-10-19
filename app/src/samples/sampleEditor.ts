// Sample editor: everything we do with one sound — waveform, slice markers, tracks from regions, saving a region to WAV.
import { WAV_FILTER } from "../api/dialogs";
import type { Id, Waveform } from "../api/types";
import { basename, joinPath, withExtension } from "../common/format";
import { regionTrack, type RegionTrackSpec } from "../tracks/samplerTracks";
import { segments, type Selection } from "./waveform";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";
import { addTracks } from "../tracks/newTracks";
import { homeDir } from "../files/paths";

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

export function createSampleEditorActions(core: Core, views: Views): SampleEditorActions {
  const { api } = core;
  return {
    waveform: (sample, from, to, buckets) => core.run(api.samples.waveform(sample, from, to, buckets)),
    setMarkers: (sample, markers) => core.exec(api.samples.setMarkers(sample, markers)),

    async extractSelection(spec, region, name) {
      if (await addTracks(core, views, [regionTrack(spec, name, region)])) core.toast("info", t().samples.trackAdded(name));
    },

    async extractSlices(spec, markers, baseName) {
      const tracks = segments(markers, spec.totalFrames).map((region, i) => regionTrack(spec, `${baseName} ${i + 1}`, region));
      if (await addTracks(core, views, tracks)) core.toast("info", t().samples.slicesCreated(tracks.length));
    },

    async saveRegion(sample, region, defaultName) {
      const path = await core.dialogs.saveFile({
        title: t().samples.saveRegionTitle,
        filters: [WAV_FILTER],
        defaultPath: joinPath(await homeDir(api), `${defaultName}${WAV_EXT}`),
      });
      if (!path) return;
      const target = withExtension(path, WAV_EXT);
      if (await core.attempt(api.samples.exportRegion(sample, region.from, region.to, target))) core.toast("info", t().samples.saved(basename(target)));
    },
  };
}
