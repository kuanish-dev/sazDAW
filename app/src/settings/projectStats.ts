import type { Project } from "../api/types";
import { songEnd } from "../tracks/clipRules";
import { barIndex, barTick, secondsAt } from "../common/tempoMap";

export interface ProjectStats {
  tracks: number;
  clips: number;
  notes: number;
  samples: number;
  graphNodes: number;
  /** Length to the end of the last clip: whole bars (a partial bar counts) and seconds. */
  bars: number;
  seconds: number;
}

/** Bars and seconds follow the project tempo map (tempo and time signature changes). */
export function projectStats(project: Project): ProjectStats {
  const end = songEnd(project.clips);
  const map = project.tempoMap;
  const fullBars = barIndex(map, end);
  return {
    tracks: project.tracks.length,
    clips: project.clips.length,
    notes: project.clips.reduce((sum, c) => sum + (c.content.type === "notes" ? c.content.notes.length : 0), 0),
    samples: project.samples.length,
    graphNodes: project.graph.nodes.length,
    bars: barTick(map, fullBars) < end ? fullBars + 1 : fullBars,
    seconds: secondsAt(map, end),
  };
}
