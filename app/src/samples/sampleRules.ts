import type { Id, Project } from "../api/types";

export interface SampleUsage {
  /** Audio clips using this sample. */
  clips: number;
  /** Sampler tracks that play it. */
  samplers: number;
}

export function sampleUsage(project: Pick<Project, "clips" | "tracks">, sample: Id): SampleUsage {
  return {
    clips: project.clips.filter((c) => c.content.type === "audio" && c.content.sample === sample).length,
    samplers: project.tracks.filter((t) => t.instrument?.params.sample === sample).length,
  };
}
