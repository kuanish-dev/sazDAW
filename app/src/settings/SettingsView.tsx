// Settings page (VS Code settings-editor feel): section list on the left, the chosen section on the right. Which section is open is session state
// (`ui.settingsSection`), so menu items ("Audio…", "Error log…") open the right one directly.
import { For, type JSX } from "solid-js";
import { Dynamic } from "solid-js/web";
import { t } from "../i18n";
import { type SettingsSection, useSession } from "../session";
import {
  IconFolder, IconInfo, IconKeyboard, IconLibrary, IconPaint, IconPerson, IconPiano, IconPlug, IconSpeaker, type IconProps,
} from "../common/icons";
import { AppearanceSection } from "./AppearanceSection";
import { AudioSection } from "./AudioSection";
import { LogSection } from "./LogSection";
import { MidiSection } from "./MidiSection";
import { PluginsSection } from "./PluginsSection";
import { ProfileSection } from "./ProfileSection";
import { ProjectSection } from "./ProjectSection";
import { ShortcutsSection } from "./ShortcutsSection";
import { SoundsSection } from "./SoundsSection";

const SECTIONS: readonly { id: SettingsSection; icon: (p: IconProps) => JSX.Element; view: () => JSX.Element }[] = [
  { id: "profile", icon: IconPerson, view: ProfileSection },
  { id: "appearance", icon: IconPaint, view: AppearanceSection },
  { id: "project", icon: IconFolder, view: ProjectSection },
  { id: "audio", icon: IconSpeaker, view: AudioSection },
  { id: "midi", icon: IconPiano, view: MidiSection },
  { id: "plugins", icon: IconPlug, view: PluginsSection },
  { id: "sounds", icon: IconLibrary, view: SoundsSection },
  { id: "shortcuts", icon: IconKeyboard, view: ShortcutsSection },
  { id: "log", icon: IconInfo, view: LogSection },
] as const;

export function SettingsView() {
  const session = useSession();
  const current = session.shell.settingsSection;

  return (
    <div class="flex h-full min-h-0">
      <nav class="flex w-52 shrink-0 flex-col overflow-y-auto border-r border-line bg-panel py-1" aria-label={t().settings.sectionsLabel}>
        <div class="section-header">{t().shell.tabs.settings}</div>
        <For each={SECTIONS}>
          {(section) => (
            <button
              class="flex h-[26px] w-full shrink-0 cursor-pointer items-center gap-2 px-3 text-left text-sm"
              classList={{ "bg-select text-text": current() === section.id, "text-muted hover:bg-hover hover:text-text": current() !== section.id }}
              aria-current={current() === section.id ? "page" : undefined}
              onClick={() => session.shell.showSettings(section.id)}
            >
              <section.icon />
              {t().settings.sections[section.id]}
            </button>
          )}
        </For>
      </nav>
      <div class="min-w-0 flex-1 overflow-y-auto">
        <div class="mx-auto max-w-3xl px-8 py-6">
          <Dynamic component={SECTIONS.find((s) => s.id === current())?.view} />
        </div>
      </div>
    </div>
  );
}
