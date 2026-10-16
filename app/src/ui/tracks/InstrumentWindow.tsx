// Floating window with the selected track's instrument parameters: doesn't cover the rest of the interface, can be dragged.
// On top — the track's MIDI effects (`NoteEffectsSection`): notes pass through them before the instrument.
import { For, Show, createMemo, createSignal } from "solid-js";
import { instrumentInfo, paramValue, paramValues } from "../../domain/catalog";
import { isPluginKind } from "../../domain/plugins";
import { SYNTH_KIND } from "../../domain/synth";
import { trackById } from "../../domain/tracks";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { IconClose } from "../common/icons";
import { ParamControl } from "../common/ParamControl";
import { PresetBar } from "../common/PresetBar";
import { capture } from "../common/pointer";
import { EnvelopeGraph } from "./EnvelopeGraph";
import { NoteEffectsSection } from "./NoteEffectsSection";
import { SynthGraph } from "./SynthGraph";

export function InstrumentWindow() {
  const session = useSession();
  const { project, selection } = session;
  const [offset, setOffset] = createSignal({ x: 0, y: 0 });
  let drag: { startX: number; startY: number; origin: { x: number; y: number } } | null = null;

  /** The selected instrument track (an audio track has no instrument parameters). */
  const track = createMemo(() => {
    const tr = trackById(project, selection.track());
    return tr?.instrument ? { ...tr, instrument: tr.instrument } : undefined;
  });
  const kind = createMemo(() => instrumentInfo(session.catalog(), track()?.instrument.kind));
  const infos = () => kind()?.params ?? [];
  const values = () => paramValues(track()?.instrument.params ?? {}, infos());

  const change = (name: string, value: number) => {
    const tr = track();
    if (tr) void session.actions.tracks.setParam(tr.id, name, value);
  };

  return (
    <aside
      class="anim-pop origin-top-right fixed right-6 top-28 z-[45] flex max-h-[calc(100vh-9rem)] w-80 flex-col rounded border border-field-line bg-panel shadow-2xl"
      style={{ transform: `translate(${offset().x}px, ${offset().y}px)` }}
      role="dialog"
      aria-label={t().tracks.instrument.title}
    >
      <div
        class="flex h-9 shrink-0 cursor-move touch-none items-center gap-2 border-b border-line pl-3 pr-1.5"
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest("button, select")) return;
          capture(e.currentTarget, e);
          drag = { startX: e.clientX, startY: e.clientY, origin: offset() };
        }}
        onPointerMove={(e) => drag && setOffset({ x: drag.origin.x + e.clientX - drag.startX, y: drag.origin.y + e.clientY - drag.startY })}
        onPointerUp={() => (drag = null)}
      >
        <h2 class="label shrink-0">{t().tracks.instrument.title}</h2>
        <select class="field !h-6 min-w-0 flex-1 text-xs" aria-label={t().tracks.instrument.track} value={selection.track() ?? ""} onChange={(e) => selection.selectTrack(Number(e.currentTarget.value))}>
          <For each={project.tracks.filter((tr) => tr.instrument)}>{(tr) => <option value={tr.id}>{tr.name}</option>}</For>
        </select>
        <button class="btn btn-icon !h-6 !w-6 !min-w-6" aria-label={t().common.close} title={t().common.close} onClick={() => session.ui.closeInstrument()}><IconClose size={14} /></button>
      </div>
      <Show when={track()} fallback={<p class="p-4 text-sm text-muted">{t().tracks.instrument.pickTrack}</p>}>
        {(tr) => (
          <div class="flex flex-col gap-3 overflow-y-auto p-3">
            <NoteEffectsSection track={tr().id} effects={tr().noteEffects ?? []} />
            <div class="flex items-center justify-between gap-2 border-t border-line pt-3">
              <span class="min-w-0 truncate text-xs text-muted">{kind()?.label ?? tr().instrument.kind}</span>
              <Show when={isPluginKind(tr().instrument.kind)}>
                <button class="btn shrink-0 text-xs" title={t().common.pluginWindowHint} onClick={() => void session.actions.pluginEditor.open({ type: "instrument", track: tr().id })}>
                  {t().common.pluginWindow}
                </button>
              </Show>
            </div>
            <PresetBar target={{ type: "instrument", track: tr().id }} kind={tr().instrument.kind} />
            <Show when={tr().instrument.kind === SYNTH_KIND}>
              <SynthGraph waveform={values().waveform ?? 0} lowCut={values().low_cut ?? 20} highCut={values().high_cut ?? 18_000} resonance={values().resonance ?? 0} />
            </Show>
            <EnvelopeGraph infos={infos()} values={values()} onChange={change} />
            <div class="grid grid-cols-4 gap-x-1 gap-y-3">
              <For each={infos()}>
                {(info) => <ParamControl info={info} value={paramValue(tr().instrument.params, info)} target={{ type: "instrumentParam", track: tr().id, param: info.name }} onChange={(v) => change(info.name, v)} />}
              </For>
            </div>
            <Show when={project.samples.length === 0 && infos().some((p) => p.kind === "sample")}>
              <span class="text-[11px] text-muted">{t().tracks.instrument.addSound}</span>
            </Show>
          </div>
        )}
      </Show>
    </aside>
  );
}
