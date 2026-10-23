import type { Id, NewTrack, SampleInfo } from "../api/types";
import { MIDDLE_C } from "../common/pitch";
import { regionToParams, type Selection } from "../samples/waveform";

/** Sampler track playing the whole sound, named after it; at middle C the sound plays at its own pitch. */
export const samplerTrack = (sample: SampleInfo): NewTrack => ({
  name: sample.name, kind: "sampler", defaultPitch: MIDDLE_C, params: { sample: sample.id, root: MIDDLE_C },
});

/** Parameters of a sampler track playing a region of a sample. */
export interface RegionTrackSpec {
  sample: Id;
  /** File length in frames: converts the region into `start`/`end` fractions. */
  totalFrames: number;
  /** Root note: at it the sound plays at its original pitch. */
  root: number;
}

/** Sampler track `label` playing region `region` of the sample. */
export function regionTrack(spec: RegionTrackSpec, label: string, region: Selection): NewTrack {
  const { start, end } = regionToParams(region, spec.totalFrames);
  return { name: label, kind: "sampler", defaultPitch: spec.root, params: { sample: spec.sample, start, end, root: spec.root } };
}
