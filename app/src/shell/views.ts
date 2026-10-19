// What the window shows: the activity bar tab, the open settings section and the instrument window, and showing what a
// role just created (it is selected once the project copy has it).
import { createSignal } from "solid-js";
import type { Id } from "../api/types";
import type { Selection } from "../selection/selection";

export type Tab = "tracks" | "graph" | "mixer" | "stage" | "samples" | "settings";

/** Something a role just made, to be selected once it is in the project copy. */
export type Created = { track: Id } | { clips: Id[] } | { sample: Id };

export type SettingsSection = "profile" | "appearance" | "project" | "audio" | "midi" | "plugins" | "sounds" | "shortcuts" | "log";

export interface Views {
  tab(): Tab;
  showTab(next: Tab): void;
  settingsSection(): SettingsSection;
  /** The "Settings" tab opened at a given section (menu items "Audio...", "Error log..."). */
  showSettings(section: SettingsSection): void;
  instrumentOpen(): boolean;
  /** Parameter window of track `track`'s instrument (that track also becomes selected). */
  openInstrument(track: Id): void;
  closeInstrument(): void;
  /**
   * Re-reads the project, then selects what was created (a track; clips and the last one's track; a sample in the
   * editor) and, with `tab`, opens that tab. Selecting before the re-read would be undone: the selection gives way to
   * items the project copy doesn't have yet.
   */
  showCreated(created: Created, tab?: Tab): Promise<void>;
}

export function createViews(selection: Selection, refresh: () => Promise<void>): Views {
  const [tab, setTab] = createSignal<Tab>("tracks");
  const [settingsSection, setSettingsSection] = createSignal<SettingsSection>("profile");
  const [instrumentOpen, setInstrumentOpen] = createSignal(false);
  return {
    tab,
    showTab: (next) => setTab(next),
    settingsSection,
    showSettings(section) {
      setSettingsSection(section);
      setTab("settings");
    },
    instrumentOpen,
    openInstrument(track) {
      selection.selectTrack(track);
      setInstrumentOpen(true);
    },
    closeInstrument: () => setInstrumentOpen(false),
    async showCreated(created, next) {
      await refresh();
      if ("track" in created) selection.selectTrack(created.track);
      else if ("clips" in created) selection.selectClips(created.clips);
      else selection.selectSample(created.sample);
      if (next) setTab(next);
    },
  };
}
