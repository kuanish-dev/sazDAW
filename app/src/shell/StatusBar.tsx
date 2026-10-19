// Status bar at the bottom of the window (as in VS Code): recording state, project name with the unsaved dot, the hover
// hint, tempo and time signature, audio device format, plugin latency, engine load and a shortcut to the error log.
// Turns red while anything records. Reads session state only; clicks go through session intents.
import { Show, createEffect, createSignal, on, type JSX } from "solid-js";
import { formatDuration } from "../common/time";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconError, IconRecord } from "../common/icons";
import { HintText } from "./HintText";

/** One status bar item; with `onClick` it is a button with the hover background. */
function Item(props: { title?: string; onClick?: () => void; children: JSX.Element }) {
  return (
    <Show when={props.onClick} fallback={<span class="flex h-full shrink-0 items-center gap-1 px-2" title={props.title}>{props.children}</span>}>
      {(click) => (
        <button class="flex h-full shrink-0 cursor-pointer items-center gap-1 px-2 hover:bg-text/10" title={props.title} onClick={() => click()()}>
          {props.children}
        </button>
      )}
    </Show>
  );
}

/** Sample rate and buffer size of the running audio device (`frames` null — the device default). */
interface AudioFormat {
  rate: number;
  frames: number | null;
}

const formatTempo = (bpm: number) => (Number.isInteger(bpm) ? String(bpm) : bpm.toFixed(1));
const formatKhz = (rate: number) => String(Math.round(rate / 100) / 10);

export function StatusBar() {
  const session = useSession();
  const tick = session.tick;
  const status = session.status;
  const msg = () => t().shell.status;
  const transportMsg = () => t().project.transport;

  // Audio format changes only on the settings page: read it at start and every time the settings page is left.
  // `undefined` — not read yet (the item is hidden), "off" — the device is not running.
  const [audio, setAudio] = createSignal<AudioFormat | "off">();
  const refreshAudio = async () => {
    const [current, settings] = await Promise.all([session.settings.audio.status(), session.settings.audio.settings()]);
    setAudio(current?.running ? { rate: current.sampleRate, frames: settings?.bufferFrames ?? null } : "off");
  };
  createEffect(on(session.shell.tab, (_tab, previous) => {
    if (previous === undefined || previous === "settings") void refreshAudio();
  }));
  const audioText = (format: AudioFormat | "off") => (format === "off" ? msg().audioOff : msg().audio(formatKhz(format.rate), format.frames));

  const recording = () => tick().record.capturing || session.recording.recordingStatus() !== null;

  return (
    <footer
      class="flex h-[22px] shrink-0 items-center border-t text-xs"
      classList={{ "border-line bg-panel text-muted": !recording(), "border-danger bg-danger text-white": recording() }}
      aria-label={msg().label}
    >
      <Show when={session.recording.recordingStatus()}>
        {(take) => (
          <Item title={transportMsg().audioRecording}>
            <IconRecord size={12} class="animate-pulse" />
            <span>{transportMsg().audioRecording} <span class="font-mono">{formatDuration(take().seconds)}</span></span>
          </Item>
        )}
      </Show>
      <Show when={tick().record.capturing && session.recording.recordingStatus() === null}>
        <Item><IconRecord size={12} class="animate-pulse" />{transportMsg().recordingNotes(tick().record.takeNotes)}</Item>
      </Show>
      <Item title={status().path ?? undefined}>
        <span class="max-w-[220px] truncate" classList={{ "text-text": !recording() }}>{status().name || t().project.defaultName}</span>
        <Show when={status().dirty}>
          <span class="h-2 w-2 rounded-full" classList={{ "bg-accent": !recording(), "bg-white": recording() }} title={transportMsg().unsaved} />
        </Show>
      </Item>
      <Show when={session.settings.appearance.showHints()} fallback={<span class="flex-1" />}>
        <HintText />
      </Show>
      <Item title={msg().tempoHint}>{msg().tempo(formatTempo(tick().tempo), tick().signature.numerator, tick().signature.denominator)}</Item>
      <Show when={audio()}>
        {(format) => <Item title={msg().audioHint} onClick={() => session.shell.showSettings("audio")}>{audioText(format())}</Item>}
      </Show>
      <Show when={tick().latencyMs > 0}>
        <Item title={transportMsg().pdcHint}>{transportMsg().pdc(Math.round(tick().latencyMs))}</Item>
      </Show>
      <Item title={transportMsg().cpuHint}><span class="w-14 font-mono">CPU {Math.round(tick().cpu * 100)}%</span></Item>
      <Item title={msg().errorLog} onClick={() => session.shell.showSettings("log")}>
        <IconError size={14} />
      </Item>
    </footer>
  );
}
