// What the window shows: the activity bar tab, the open settings section and the instrument window.
import { createSignal } from "solid-js";
import type { Tab } from "../actions";
import type { Id } from "../api/types";

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
}

export function createViews(selectTrack: (id: Id) => void): Views {
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
      selectTrack(track);
      setInstrumentOpen(true);
    },
    closeInstrument: () => setInstrumentOpen(false),
  };
}
