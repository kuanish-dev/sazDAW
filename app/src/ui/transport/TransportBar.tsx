// Transport strip at the top of the window (compact, like VS Code's title bar): project menu, playback (the Play button pulses
// on every beat), position with beat lights (`BeatLights.tsx`), tempo
// and time signature, the song loop, metronome, recording, the audio-recording badge, the mono-check badge, and undo/redo with the history panel (`HistoryPanel.tsx`). The theme lives in Settings → Appearance. Engine load,
// latency and the audio format live in the status bar (`ui/shell/StatusBar.tsx`).
import { createMemo, createSignal, For, Show } from "solid-js";
import { barPosition, secondsAt } from "../../domain/tempoMap";
import { MAX_NUMERATOR, MAX_TEMPO, MIN_TEMPO, RECORD_SNAPS, SIGNATURE_DENOMINATORS, formatBarPosition, formatDuration, formatSeconds } from "../../domain/time";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import {
  IconChevronDown, IconLoop, IconMetronome, IconPause, IconPlay, IconRecord, IconRedo, IconStop, IconUndo,
} from "../common/icons";
import { NumberField } from "../common/NumberField";
import { ToolbarDivider } from "../common/Toolbar";
import { BeatLights, createBeat } from "./BeatLights";
import { HistoryButton } from "./HistoryPanel";
import { ProjectMenu } from "./ProjectMenu";

/** Red "recording audio" badge with a Stop button: visible on every view. */
function AudioRecordBadge() {
  const session = useSession();
  return (
    <Show when={session.recordingStatus()}>
      {(s) => (
        <div class="flex h-7 shrink-0 items-center gap-2 rounded bg-danger pl-2 pr-0.5 text-xs font-medium text-white" role="status">
          <IconRecord size={12} class="animate-pulse" />
          <span>{t().project.transport.audioRecording} <span class="font-mono">{formatDuration(s().seconds)}</span></span>
          <button class="btn !h-6 !bg-black/25 !px-2 text-xs !text-white hover:!bg-black/40" onClick={() => void session.actions.audioRecord.stop()}>
            {t().project.transport.stop}
          </button>
        </div>
      )}
    </Show>
  );
}

/** The mono check is on (switched in the mixer's meters): a badge on every view, so it is never left on unnoticed; a
 *  click turns it off. */
function MonoBadge() {
  const session = useSession();
  return (
    <Show when={session.tick().mono}>
      <button class="btn btn-on !h-7 shrink-0 text-xs font-semibold" title={t().mixer.meters.monoOnHint} onClick={() => void session.actions.audio.setMono(false)}>
        {t().mixer.meters.mono}
      </button>
    </Show>
  );
}

export function TransportBar() {
  const session = useSession();
  const { transport, tempo, history, noteRecord } = session.actions;
  const tick = session.tick;
  const status = session.status;

  // Bar and time follow the tempo map (tempo and signature changes); the tempo and signature fields show the playhead's.
  const position = createMemo(() => formatBarPosition(barPosition(session.project.tempoMap, tick().positionTicks)));
  const seconds = createMemo(() => formatSeconds(secondsAt(session.project.tempoMap, tick().positionTicks)));
  const here = () => tick().positionTicks;
  const playing = () => tick().state === "playing";
  const beat = createBeat();
  const [menuOpen, setMenuOpen] = createSignal(false);
  const record = () => tick().record;
  const audioTake = () => session.recordingStatus();
  const recordLit = () => record().armed || audioTake() !== null;

  const msg = () => t().project.transport;
  const undoTitle = () => {
    const label = status().undoLabel;
    return label ? msg().undoWhat(label) : msg().undo;
  };

  return (
    <header class="flex min-h-10 flex-wrap items-center gap-x-1.5 gap-y-1 border-b border-line bg-panel px-2 py-1.5">
      <div class="relative">
        <button class="btn btn-ghost max-w-[240px] font-medium" onClick={() => setMenuOpen(!menuOpen())} aria-expanded={menuOpen()}>
          <span class="truncate">{status().name || t().project.defaultName}</span>
          <Show when={status().dirty}><span class="h-2 w-2 shrink-0 rounded-full bg-accent" title={msg().unsaved} /></Show>
          <IconChevronDown size={14} class="text-muted" />
        </button>
        <Show when={menuOpen()}>
          <ProjectMenu onClose={() => setMenuOpen(false)} />
        </Show>
      </div>

      <ToolbarDivider />

      <div class="flex shrink-0 items-center gap-0.5">
        <button class="btn btn-icon" classList={{ "btn-on": playing(), "beat-ring-a": playing() && beat()?.flip === true, "beat-ring-b": playing() && beat()?.flip === false }} title={msg().play} aria-label={msg().play} onClick={() => void transport.play()}><IconPlay /></button>
        <button class="btn btn-icon" classList={{ "btn-on": tick().state === "paused" }} title={msg().pause} aria-label={msg().pause} onClick={() => void transport.pause()}><IconPause /></button>
        <button class="btn btn-icon" title={msg().stop} aria-label={msg().stop} onClick={() => void transport.stop()}><IconStop /></button>
        <button
          class="btn btn-ghost !px-2"
          classList={{ "!bg-danger !text-white": recordLit(), "text-danger": !recordLit(), "animate-pulse": record().capturing || audioTake() !== null }}
          aria-pressed={record().armed}
          title={msg().recordHint}
          aria-label={msg().record}
          onClick={() => void session.actions.audioRecord.pressRecord(record().armed)}
        >
          <IconRecord />
          <Show when={record().capturing}><span class="text-xs">{msg().recordingNotes(record().takeNotes)}</span></Show>
        </button>
        <Show when={record().armed}>
          <select class="field ml-1 w-28 text-xs" title={msg().quantizeHint} aria-label={msg().quantize} value={record().quantizeTicks} onChange={(e) => void noteRecord.setQuantize(Number(e.currentTarget.value))}>
            <For each={RECORD_SNAPS}>{(snap) => <option value={snap.ticks}>{snap.label}</option>}</For>
          </select>
        </Show>
      </div>

      <div class="flex h-7 shrink-0 items-baseline gap-2 rounded border border-field-line bg-bg px-2.5 leading-[26px]" title={msg().position}>
        <span class="font-mono text-[15px] font-semibold tabular-nums text-accent">{position()}</span>
        <span class="font-mono text-[11px] tabular-nums text-muted">{seconds()}</span>
        <BeatLights beat={beat()} />
      </div>

      <label class="flex shrink-0 items-center gap-1.5 pl-1" title={msg().tempoHint}>
        <span class="label">BPM</span>
        <NumberField value={tick().tempo} min={MIN_TEMPO} max={MAX_TEMPO} step={1} decimals={1} class="w-16" label={msg().tempo} onCommit={(v) => void tempo.setTempoAt(here(), v)} />
      </label>

      <div class="flex shrink-0 items-center gap-1" title={msg().signatureHint}>
        <span class="label pl-1">{msg().signature}</span>
        <NumberField value={tick().signature.numerator} min={1} max={MAX_NUMERATOR} class="w-10" label={msg().numerator} onCommit={(v) => void tempo.setSignatureAt(here(), { numerator: v, denominator: tick().signature.denominator })} />
        <span class="text-muted">/</span>
        <select class="field w-12" aria-label={msg().denominator} value={tick().signature.denominator} onChange={(e) => void tempo.setSignatureAt(here(), { numerator: tick().signature.numerator, denominator: Number(e.currentTarget.value) })}>
          {SIGNATURE_DENOMINATORS.map((d) => <option value={d}>{d}</option>)}
        </select>
      </div>

      <button
        class="btn btn-icon" classList={{ "btn-on": tick().songLoop }} aria-pressed={tick().songLoop}
        title={msg().songLoopHint} aria-label={msg().songLoop} onClick={() => void transport.setSongLoop(!tick().songLoop)}
      ><IconLoop /></button>

      <button
        class="btn btn-icon" classList={{ "btn-on": tick().metronome }} aria-pressed={tick().metronome}
        title={msg().metronome} aria-label={msg().metronome} onClick={() => void transport.setMetronome(!tick().metronome)}
      ><IconMetronome /></button>

      <AudioRecordBadge />
      <MonoBadge />

      <div class="ml-auto flex shrink-0 items-center gap-0.5">
        <button class="btn btn-icon" disabled={!status().canUndo} title={undoTitle()} aria-label={msg().undo} onClick={() => void history.undo()}><IconUndo /></button>
        <button class="btn btn-icon" disabled={!status().canRedo} title={msg().redo} aria-label={msg().redo} onClick={() => void history.redo()}><IconRedo /></button>
        <HistoryButton />
      </div>
    </header>
  );
}
