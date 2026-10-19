// Settings → Sounds & packs: linked folders to take sounds from (the sound browser shows them under Packs too).
import { For, createSignal, onMount } from "solid-js";
import { t } from "../i18n";
import { SectionTitle, SettingGroup } from "./SettingRow";
import { useSounds } from "../sounds/role";

export function SoundsSection() {
  const { browser } = useSounds();
  const [sources, setSources] = createSignal<string[]>([]);

  const reload = async () => setSources(await browser.soundSources());
  onMount(reload);

  const add = async () => {
    if (await browser.addSoundSource()) await reload();
  };
  const remove = async (path: string) => {
    await browser.removeSoundSource(path);
    await reload();
  };

  return (
    <>
      <SectionTitle title={t().settings.sections.sounds} hint={t().settings.sounds.hint} />
      <SettingGroup title={t().settings.sounds.packs}>
        <For each={sources()} fallback={<p class="px-4 py-3 text-sm text-muted">{t().settings.sounds.noPacks}</p>}>
          {(path) => (
            <div class="flex items-center gap-3 px-4 py-2">
              <span class="min-w-0 flex-1 truncate font-mono text-xs" title={path}>{path}</span>
              <button class="btn !h-7 text-xs hover:text-danger" title={t().settings.sounds.removeHint} onClick={() => void remove(path)}>{t().common.remove}</button>
            </div>
          )}
        </For>
      </SettingGroup>
      <button class="btn" onClick={() => void add()}>{t().settings.sounds.addFolder}</button>
    </>
  );
}
