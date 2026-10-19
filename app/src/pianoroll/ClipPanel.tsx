// Panel of the selected clip under the timeline: a note clip gets a header (track and length, "Loop clip", content length,
// view switch) and one of the views (Piano roll or Steps); an audio clip gets its properties (`AudioClipPanel`); an
// automation clip gets its pattern editor (`automation/AutomationClipPanel`).
// This file knows what is selected and which view is shown; editing lives in the views themselves.
import { Match, Show, Switch, createSignal } from "solid-js";
import { MAX_CLIP_LOOP_TICKS, type TempoMap, type Track } from "../api/types";
import { automationClipOf } from "../automation/automationRules";
import { asNoteClip, clipBarTicks, type NoteClip } from "./noteClip";
import { MIDDLE_C } from "../common/pitch";
import { barsBetween } from "../common/tempoMap";
import { clipById, trackById } from "../tracks/trackRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { AutomationClipPanel } from "../automation/AutomationClipPanel";
import { BarsStepper } from "../common/BarsStepper";
import { IconLoop, IconRoll, IconSteps } from "../common/icons";
import { sticky } from "../common/sticky";
import { ToolButton } from "../common/Toolbar";
import { AudioClipPanel } from "./AudioClipPanel";
import { PianoRoll } from "./PianoRoll";
import { StepGrid } from "./StepGrid";

type ClipView = "roll" | "steps";

const VIEWS: readonly ClipView[] = ["roll", "steps"] as const;

/** "Content length" in bars: how many bars of notes repeat inside the clip. Changing it by hand turns off auto length. */
function ContentLength(props: { clip: NoteClip }) {
  const session = useSession();
  const { pianoroll } = session;
  return (
    <BarsStepper
      label={t().pianoRoll.contentLength} hint={t().pianoRoll.contentLengthHint} ticks={props.clip.content.loopTicks}
      bar={clipBarTicks(session.project.tempoMap, props.clip)} max={MAX_CLIP_LOOP_TICKS}
      onSet={(ticks) => void pianoroll.setLoopTicks(props.clip.id, ticks)}
    />
  );
}

/** Clip length on the timeline in song bars (by the tempo map), to hundredths. */
const clipBars = (clip: NoteClip, map: TempoMap) => Math.round(barsBetween(map, clip.startTick, clip.startTick + clip.lengthTicks) * 100) / 100;

function ClipPanelHeader(props: { clip: NoteClip; track: Track | undefined; view: ClipView; onView(view: ClipView): void }) {
  const session = useSession();
  return (
    <div class="flex min-h-9 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line py-1 pl-3 pr-1.5">
      <h2 class="label">{props.track?.name ?? t().pianoRoll.trackFallback}</h2>
      <span class="text-xs text-muted">{t().pianoRoll.clipBars(clipBars(props.clip, session.project.tempoMap))}</span>
      <ToolButton icon={<IconLoop />} label={t().pianoRoll.loopClip} title={t().pianoRoll.loopClipHint} text onClick={() => void session.transport.loopClip(props.clip)} />
      <ContentLength clip={props.clip} />
      <div class="ml-auto flex rounded border border-field-line p-px" role="tablist" aria-label={t().pianoRoll.clipView}>
        {VIEWS.map((v) => (
          <button class="btn btn-ghost !h-6 gap-1.5 !px-2 text-xs" classList={{ "btn-on": props.view === v, "text-muted": props.view !== v }} role="tab" aria-selected={props.view === v} onClick={() => props.onView(v)}>
            {v === "roll" ? <IconRoll size={14} /> : <IconSteps size={14} />}{t().pianoRoll.views[v]}
          </button>
        ))}
      </div>
    </div>
  );
}

/** `snap` is the timeline snap grid (ticks), used by the take lanes of an audio clip. */
export function ClipPanel(props: { snap: number }) {
  const session = useSession();
  const [view, setView] = createSignal<ClipView>("roll");
  const selected = () => clipById(session.project, session.selection.clip());
  const clip = () => asNoteClip(selected());
  const audio = () => (selected()?.content.type === "audio" ? selected() : undefined);
  const autoClip = () => automationClipOf(session.project, session.selection.automation());
  const track = () => trackById(session.project, selected()?.track ?? null);
  // Children read "sticky" values: when the clip is deleted they disappear instead of crashing on a stale value.
  const lastClip = sticky(clip);
  const lastTrack = sticky(track);

  return (
    <section class="panel flex h-[45rem] min-h-0 min-w-0 flex-col" aria-label={t().pianoRoll.panel}>
      <Show when={audio()}>
        {(a) => <AudioClipPanel clip={a()} track={track()} snap={props.snap} />}
      </Show>
      <Show when={autoClip()}>
        {(a) => <AutomationClipPanel lane={a().lane} clip={a().clip} snap={props.snap} />}
      </Show>
      <Show
        when={clip()}
        fallback={<Show when={!audio() && !autoClip()}><p class="p-4 text-sm text-muted">{t().pianoRoll.noClip}</p></Show>}
      >
        <ClipPanelHeader clip={lastClip()!} track={lastTrack()} view={view()} onView={setView} />
        <Switch>
          <Match when={view() === "roll"}>
            <div class="min-h-0 flex-1"><PianoRoll clip={lastClip()!} defaultPitch={lastTrack()?.defaultPitch ?? MIDDLE_C} /></div>
          </Match>
          <Match when={view() === "steps" && track()}>
            <StepGrid clip={lastClip()!} track={lastTrack()!} />
          </Match>
        </Switch>
      </Show>
    </section>
  );
}
