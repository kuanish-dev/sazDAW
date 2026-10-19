// Audio track input in its header: record arm (red — armed, blinking — a take is running), input channel choice
// (the chevron, or right-click on the arm button), input level and take time when armed, input monitor button "IN".
import { createResource, createSignal, For, Show } from "solid-js";
import type { Id, InputChannels, MonitorMode } from "../api/types";
import { plainLevel } from "../mixer/mixer";
import { hearsInput, INPUT_SHORT, inputChoices, nextMonitorMode, selectedInputChannels } from "../recording/recordInput";
import { formatDuration } from "../common/time";
import { t } from "../i18n";
import { ContextMenu, MenuCheck, MenuItem, MenuLabel, type MenuPoint } from "../common/ContextMenu";
import { IconChevronDown, IconRecord } from "../common/icons";
import { Meter } from "../common/Meter";
import { useRecording } from "../recording/role";
import { useSettings } from "../settings/role";
import { useTracks } from "./role";

/**
 * "Track input" menu: input channels available on the selected input device. The shared menu (`ContextMenu`) floats
 * above everything at the button, so the track header doesn't grow.
 */
function InputMenu(props: { track: Id; current: InputChannels; at: MenuPoint; onClose: () => void }) {
  const { audio } = useSettings();
const { audioRecord } = useRecording();
  const [channels] = createResource(async () => {
    const [devices, settings] = await Promise.all([audio.devices(), audio.settings()]);
    return devices && settings ? selectedInputChannels(devices, settings) : 0;
  });

  const pick = (input: InputChannels) => {
    if (input !== props.current) void audioRecord.setInput(props.track, input);
  };

  return (
    <ContextMenu at={props.at} label={t().tracks.input.title} onClose={props.onClose}>
      <MenuLabel>{t().tracks.input.title}</MenuLabel>
      <For each={inputChoices(channels() ?? 0)}>
        {(input) => (
          <MenuItem onClose={props.onClose} onPick={() => pick(input)}>
            <MenuCheck on={input === props.current}>{t().tracks.inputs[input]}</MenuCheck>
          </MenuItem>
        )}
      </For>
      <Show when={channels() === 1}>
        <div class="px-2.5 py-1 text-[11px] text-muted">{t().tracks.input.mono}</div>
      </Show>
    </ContextMenu>
  );
}

export function RecordButton(props: { track: Id }) {
  const recording = useRecording();
  const { audioRecord } = recording;
  const input = () => recording.trackInput(props.track);
  const take = () => recording.recordingStatus();
  const recordingHere = () => take() !== null && input().armed;
  /** Where to open the input menu (below the button), or null — closed. */
  const [menuAt, setMenuAt] = createSignal<{ x: number; y: number } | null>(null);
  const openMenu = (button: HTMLElement) => {
    const rect = button.getBoundingClientRect();
    setMenuAt({ x: rect.left, y: rect.bottom + 2 });
  };

  const title = () =>
    recordingHere()
      ? t().tracks.input.stop
      : t().tracks.input.armHint(input().armed, t().tracks.inputs[input().input]);

  return (
    <>
      <div class="flex shrink-0 items-center">
        <button
          class="btn !h-6 !min-w-6 !rounded-r-none !px-0 text-danger"
          classList={{ "!bg-danger !text-white": input().armed, "animate-pulse": recordingHere() }}
          disabled={take() !== null && !input().armed} aria-pressed={input().armed} title={title()} aria-label={title()}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={() => void (recordingHere() ? audioRecord.stop() : audioRecord.setArmed(props.track, !input().armed))}
          onContextMenu={(e) => { e.preventDefault(); if (take() === null) openMenu(e.currentTarget); }}
        ><IconRecord size={10} /></button>
        <button
          class="btn !h-6 !min-w-0 !gap-0 !rounded-l-none border-l !border-l-line !px-0.5 font-mono text-[9px] text-muted"
          disabled={take() !== null} title={t().tracks.input.current(t().tracks.inputs[input().input])} aria-label={t().tracks.input.pick}
          onPointerDown={(e) => e.stopPropagation()} onClick={(e) => openMenu(e.currentTarget)}
        >{INPUT_SHORT[input().input]}<IconChevronDown size={10} /></button>
      </div>
      <Show when={recordingHere() ? take() : null}>
        {(s) => <span class="shrink-0 font-mono text-[10px] text-danger">{formatDuration(s().seconds)}</span>}
      </Show>
      <Show when={input().armed}>
        <Meter level={plainLevel(input().left, input().right)} height={16} />
      </Show>
      <Show when={menuAt()}>
        {(at) => <InputMenu track={props.track} current={input().input} at={at()} onClose={() => setMenuAt(null)} />}
      </Show>
    </>
  );
}

/**
 * Input monitoring: a click cycles off → auto → on. The input goes through the track's fader and effects; with speakers
 * near the microphone there will be feedback — the tooltip warns about it.
 */
export function MonitorButton(props: { track: Id; mode: MonitorMode }) {
  const recording = useRecording();
  const tracks = useTracks();
  const armed = () => recording.trackInput(props.track).armed;
  /** The track should hear its input but it's silent (sample rates differ, the input didn't open). */
  const problem = () => (hearsInput(props.mode, armed()) ? recording.monitorProblem() : null);
  const title = () =>
    [
      t().tracks.input.monitor(t().tracks.monitorModes[props.mode]),
      t().tracks.input.feedbackWarning,
      problem() ?? "",
    ].filter(Boolean).join("\n");

  return (
    <button
      class="btn !h-6 !min-w-6 !px-0 text-[10px] font-semibold"
      classList={{ "btn-on": props.mode === "on", "text-muted": props.mode === "off", "ring-1 ring-inset ring-accent text-accent": props.mode === "auto", "!text-danger": problem() !== null }}
      aria-label={title()} title={title()} aria-pressed={props.mode !== "off"}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={() => void tracks.setMonitor(props.track, nextMonitorMode(props.mode))}
    >{props.mode === "auto" ? "IN·A" : "IN"}</button>
  );
}
