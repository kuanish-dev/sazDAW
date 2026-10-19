// Settings → Appearance: language, theme, accent color, interface scale, track height and hint bar.
// `settings/appearance.ts` applies them; this section only changes preferences through the action.
import { For, createSignal } from "solid-js";
import type { AppearancePreferences } from "../api/types";
import { ACCENT_CHOICES, THEME_CHOICES, TRACK_HEIGHT_CHOICES, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP } from "./preferenceRules";
import { LANGUAGE_CHOICES, t } from "../i18n";
import { useSession } from "../session";
import { SectionTitle, SettingGroup, SettingRow, Toggle } from "./SettingRow";

const percent = (scale: number) => `${Math.round(scale * 100)} %`;

/** A row of "pick one" buttons. */
function Segmented<T extends string>(props: { options: readonly { value: T; label: string }[]; value: T; label: string; onChange: (value: T) => void }) {
  return (
    <div class="flex gap-0.5 rounded border border-field-line bg-field p-0.5" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <button
            role="radio" aria-checked={props.value === o.value}
            class="btn btn-ghost !h-6 text-xs" classList={{ "btn-on": props.value === o.value, "text-muted": props.value !== o.value }}
            onClick={() => props.onChange(o.value)}
          >{o.label}</button>
        )}
      </For>
    </div>
  );
}

export function AppearanceSection() {
  const session = useSession();
  const look = () => session.settings.preferences.current().appearance;
  const msg = () => t().appearance;
  const set = (patch: Partial<AppearancePreferences>) =>
    void session.settings.preferences.update((p) => ({ ...p, appearance: { ...p.appearance, ...patch } }));
  // While the slider is dragged we show the number and change the scale on release: otherwise the page slides away from under the mouse.
  const [scaleDraft, setScaleDraft] = createSignal<number | null>(null);
  const themes = () => THEME_CHOICES.map((value) => ({ value, label: msg().themes[value] }));
  const heights = () => TRACK_HEIGHT_CHOICES.map((value) => ({ value, label: msg().trackHeights[value] }));

  return (
    <>
      <SectionTitle title={msg().title} hint={msg().hint} />
      <SettingGroup title={msg().language}>
        <SettingRow label={msg().language} hint={msg().languageHint}>
          <Segmented
            label={msg().language} options={LANGUAGE_CHOICES} value={session.settings.preferences.current().language}
            onChange={(language) => void session.settings.preferences.update((p) => ({ ...p, language }))}
          />
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().colors}>
        <SettingRow label={msg().theme} hint={look().theme === "system" ? msg().themeNow(session.settings.appearance.theme() === "dark") : undefined}>
          <Segmented label={msg().theme} options={themes()} value={look().theme} onChange={(theme) => set({ theme })} />
        </SettingRow>
        <SettingRow label={msg().accent} hint={msg().accentHint}>
          <div class="flex gap-2" role="radiogroup" aria-label={msg().accent}>
            <For each={ACCENT_CHOICES}>
              {(a) => (
                <button
                  role="radio" aria-checked={look().accent === a.value} title={msg().accents[a.value]} aria-label={msg().accents[a.value]}
                  class="h-5 w-5 cursor-pointer rounded-full ring-offset-2 ring-offset-bg"
                  classList={{ "ring-2 ring-text": look().accent === a.value }}
                  style={{ background: a.swatch }}
                  onClick={() => set({ accent: a.value })}
                />
              )}
            </For>
          </div>
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().sizes}>
        <SettingRow label={msg().uiScale} hint={msg().uiScaleHint(percent(UI_SCALE_MIN), percent(UI_SCALE_MAX))}>
          <input
            type="range" class="w-44" aria-label={msg().uiScale}
            min={UI_SCALE_MIN} max={UI_SCALE_MAX} step={UI_SCALE_STEP} value={scaleDraft() ?? look().uiScale}
            onInput={(e) => setScaleDraft(Number(e.currentTarget.value))}
            onChange={(e) => {
              // Read the value before clearing the draft: clearing re-renders the slider at the old scale.
              const uiScale = Number(e.currentTarget.value);
              setScaleDraft(null);
              set({ uiScale });
            }}
          />
          <span class="w-12 text-right font-mono text-xs">{percent(scaleDraft() ?? look().uiScale)}</span>
          <button class="btn !h-7 text-xs" disabled={look().uiScale === 1} onClick={() => set({ uiScale: 1 })}>100 %</button>
        </SettingRow>
        <SettingRow label={msg().trackHeight} hint={msg().trackHeightHint}>
          <Segmented label={msg().trackHeight} options={heights()} value={look().trackHeight} onChange={(trackHeight) => set({ trackHeight })} />
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={msg().help}>
        <SettingRow label={msg().hints} hint={msg().hintsHint}>
          <Toggle label={msg().hints} checked={look().showHints} onChange={(showHints) => set({ showHints })} />
        </SettingRow>
      </SettingGroup>
    </>
  );
}
