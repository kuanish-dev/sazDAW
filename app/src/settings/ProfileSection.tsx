// Settings → Profile: author name, default projects folder, opening the last project and autosave.
import { For, Show } from "solid-js";
import type { ProfilePreferences } from "../api/types";
import { AUTOSAVE_CHOICES, autosaveLabel } from "./preferenceRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { SectionTitle, SettingGroup, SettingRow, Toggle } from "./SettingRow";
import { IconClose } from "../common/icons";

export function ProfileSection() {
  const session = useSession();
  const { preferences } = session.settings;
  const profile = () => session.settings.preferences.current().profile;
  const msg = () => t().profile;
  const setProfile = (patch: Partial<ProfilePreferences>) =>
    void preferences.update((p) => ({ ...p, profile: { ...p.profile, ...patch } }));
  /** A value outside the list (set elsewhere) is shown too, so the select doesn't jump. */
  const autosaveChoices = () => {
    const current = session.settings.preferences.current().autosaveSeconds;
    return AUTOSAVE_CHOICES.includes(current) ? AUTOSAVE_CHOICES : [...AUTOSAVE_CHOICES, current].sort((a, b) => a - b);
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.profile} hint={msg().hint} />
      <SettingGroup title={msg().author}>
        <SettingRow label={msg().authorName} hint={msg().authorNameHint}>
          <input
            class="field w-64" placeholder={msg().authorPlaceholder} value={profile().author} aria-label={msg().authorName}
            onChange={(e) => setProfile({ author: e.currentTarget.value })}
            onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          />
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().projects}>
        <SettingRow label={msg().projectsFolder} hint={msg().projectsFolderHint}>
          <span class="max-w-64 truncate font-mono text-xs text-muted" title={profile().projectsFolder ?? undefined}>
            {profile().projectsFolder ?? msg().documents}
          </span>
          <button class="btn" onClick={() => void preferences.chooseProjectsFolder()}>{msg().choose}</button>
          <Show when={profile().projectsFolder}>
            <button class="btn btn-icon" title={msg().resetToDocuments} aria-label={msg().resetToDocuments} onClick={() => setProfile({ projectsFolder: null })}><IconClose size={14} /></button>
          </Show>
        </SettingRow>
        <SettingRow label={msg().openLast} hint={msg().openLastHint}>
          <Toggle label={msg().openLast} checked={profile().openLastProject} onChange={(on) => setProfile({ openLastProject: on })} />
        </SettingRow>
        <SettingRow label={msg().autosave} hint={msg().autosaveHint}>
          <select
            class="field w-40" aria-label={msg().autosaveInterval} value={session.settings.preferences.current().autosaveSeconds}
            onChange={(e) => {
              const autosaveSeconds = Number(e.currentTarget.value);
              void preferences.update((p) => ({ ...p, autosaveSeconds }));
            }}
          >
            <For each={autosaveChoices()}>{(s) => <option value={s}>{autosaveLabel(s)}</option>}</For>
          </select>
        </SettingRow>
      </SettingGroup>
    </>
  );
}
