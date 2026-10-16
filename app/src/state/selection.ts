// UI selection: the track (the keyboard piano plays on it), selected clips (note/audio, or one automation clip) and the
// sample in the editor.
import { createSignal } from "solid-js";
import type { Id, Project } from "../api/types";
import type { AutomationSelection } from "../domain/automation";

export interface Selection {
  /** The selected track (the keyboard piano plays on it); `null` — none. */
  track(): Id | null;
  clips(): Id[];
  /** The "single selected" clip: clip panels work only when exactly one clip is selected. */
  clip(): Id | null;
  /** The sample open in the editor. */
  sample(): Id | null;
  /** The selected automation lane row / clip; selecting it clears the clip selection. */
  automation(): AutomationSelection | null;
  selectAutomation(next: AutomationSelection | null): void;
  selectTrack(id: Id | null): void;
  /**
   * Selects clips and the track `withTrack`, or, if it is not given, the track of the last clip
   * (a clip not yet in the project selects only itself).
   */
  selectClips(ids: Id[], withTrack?: Id): void;
  clear(): void;
  selectSample(id: Id | null): void;
  /** After the project is re-read, vanished selection gives way: the track to the first one, clips to the remaining ones. */
  prune(next: Project): void;
}

export function createSelection(project: Pick<Project, "clips">): Selection {
  const [track, setTrack] = createSignal<Id | null>(null);
  const [clips, setClips] = createSignal<Id[]>([]);
  const [sample, setSample] = createSignal<Id | null>(null);
  const [automation, setAutomation] = createSignal<AutomationSelection | null>(null);

  const selectClips = (ids: Id[], withTrack?: Id) => {
    setClips(ids);
    // One clip panel: note/audio clips and an automation clip are never selected together.
    setAutomation(null);
    const track = withTrack ?? project.clips.find((c) => c.id === ids[ids.length - 1])?.track;
    if (track !== undefined) setTrack(track);
  };

  return {
    track,
    clips,
    clip: () => (clips().length === 1 ? clips()[0]! : null),
    sample,
    automation,
    selectAutomation(next) {
      setAutomation(next);
      if (next) setClips([]);
    },
    selectTrack: (id) => setTrack(id),
    selectClips,
    clear: () => setClips([]),
    selectSample: (id) => setSample(id),
    prune(next) {
      if (!next.tracks.some((t) => t.id === track())) setTrack(next.tracks[0]?.id ?? null);
      const alive = clips().filter((id) => next.clips.some((c) => c.id === id));
      if (alive.length !== clips().length) setClips(alive);
      const auto = automation();
      const lane = auto ? next.automation.find((l) => l.id === auto.lane) : undefined;
      if (auto && !lane) setAutomation(null);
      else if (auto && lane && auto.clip !== null && !lane.clips.some((c) => c.id === auto.clip)) setAutomation({ lane: auto.lane, clip: null });
    },
  };
}
