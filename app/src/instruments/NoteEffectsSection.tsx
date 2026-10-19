// The instrument window's "MIDI effects" section: the track's chain before the instrument (arpeggiator, chord) — add,
// bypass, reorder, remove and each effect's parameters. Everything goes through `session.noteEffects`.
import { For, Show } from "solid-js";
import type { Id, NoteEffectSpec } from "../api/types";
import { kindChoices, noteEffectInfo, paramValue } from "../common/catalog";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconArrowDown, IconArrowUp, IconClose, IconPower } from "../common/icons";
import { ParamControl } from "../common/ParamControl";
import { PickSelect } from "../common/PickSelect";

export function NoteEffectsSection(props: { track: Id; effects: NoteEffectSpec[] }) {
  const session = useSession();
  const actions = session.noteEffects;
  const kinds = () => kindChoices(session.catalog()?.noteEffects ?? []);

  const Effect = (item: { spec: NoteEffectSpec; index: number }) => {
    const info = () => noteEffectInfo(session.catalog(), item.spec.kind);
    const on = () => !item.spec.bypass;
    return (
      <div class="flex flex-col gap-2 rounded border border-line bg-bg p-2" classList={{ "opacity-60": !on() }}>
        <div class="flex items-center gap-1">
          <button
            type="button" class="btn btn-icon !h-6 !w-6 !min-w-6" classList={{ "btn-on !text-on-accent": on() }} aria-pressed={on()}
            title={on() ? t().tracks.noteEffects.bypassHint : t().tracks.noteEffects.enable} aria-label={t().tracks.noteEffects.bypass}
            onClick={() => void actions.setBypass(props.track, item.spec.id, on())}
          ><IconPower size={14} /></button>
          <span class="min-w-0 flex-1 truncate text-xs">{info()?.label ?? item.spec.kind}</span>
          <button type="button" class="btn btn-icon !h-6 !w-6 !min-w-6" title={t().tracks.noteEffects.earlierHint} aria-label={t().tracks.noteEffects.earlier} disabled={item.index === 0}
            onClick={() => void actions.move(props.track, item.spec.id, item.index - 1)}><IconArrowUp size={14} /></button>
          <button type="button" class="btn btn-icon !h-6 !w-6 !min-w-6" title={t().tracks.noteEffects.laterHint} aria-label={t().tracks.noteEffects.later} disabled={item.index === props.effects.length - 1}
            onClick={() => void actions.move(props.track, item.spec.id, item.index + 1)}><IconArrowDown size={14} /></button>
          <button type="button" class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger" title={t().tracks.noteEffects.removeHint} aria-label={t().common.remove}
            onClick={() => void actions.remove(props.track, item.spec.id)}><IconClose size={14} /></button>
        </div>
        <div class="grid grid-cols-4 gap-x-1 gap-y-3">
          <For each={info()?.params ?? []}>
            {(param) => (
              <ParamControl info={param} value={paramValue(item.spec.params, param)}
                onChange={(v) => void actions.setParam(props.track, item.spec.id, param.name, v)} />
            )}
          </For>
        </div>
      </div>
    );
  };

  return (
    <section class="flex flex-col gap-2" aria-label={t().tracks.noteEffects.title}>
      <div class="flex items-center gap-2">
        <h3 class="label min-w-0 flex-1">{t().tracks.noteEffects.title}</h3>
        <PickSelect class="w-40 text-xs" options={kinds()} placeholder={t().tracks.noteEffects.add}
          title={t().tracks.noteEffects.addHint} onPick={(kind) => void actions.add(props.track, kind)} />
      </div>
      <Show when={props.effects.length > 0} fallback={<span class="text-[11px] text-muted">{t().tracks.noteEffects.empty}</span>}>
        <For each={props.effects}>{(spec, index) => <Effect spec={spec} index={index()} />}</For>
      </Show>
    </section>
  );
}
