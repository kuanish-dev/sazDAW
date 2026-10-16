// Toolbar strip above the timeline: tools (icons), "+ Track", drum and chord parts, grid, loop, the live automation write mode and
// the selected audio clip's properties.
import { For, type JSX, Show, createSignal } from "solid-js";
import { STEP_TICKS, type AutomationWriteMode } from "../../api/types";
import { kindChoices } from "../../domain/catalog";
import { GRID_CHOICES } from "../../domain/time";
import { MAX_CLIP_GAIN } from "../../domain/clips";
import { clipById } from "../../domain/tracks";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { IconDrum, IconEraser, IconFlag, IconGuitar, IconLoop, IconPencil, IconScissors } from "../common/icons";
import { PickSelect } from "../common/PickSelect";
import { ToolButton, Toolbar, ToolbarDivider } from "../common/Toolbar";
import { ChordPartDialog } from "../chords/ChordPartDialog";
import { DrumPartDialog } from "../drums/DrumPartDialog";
import type { Tool } from "./timelineLayout";

/** Clip gain in percent on the slider (up to the backend's maximum, ×4). */
const MAX_GAIN_PERCENT = MAX_CLIP_GAIN * 100;
/** Clip fades: up to the clip length, in sixty-fourth steps (ticks); the handles on the clip set them too. */
const FADE_STEP_TICKS = STEP_TICKS / 4;
const AUDIO_TRACK = "__audio__";
const WRITE_MODES: AutomationWriteMode[] = ["off", "touch", "latch"];

/** Toolbar state shared with the timeline and automation: tool, grid, zoom. */
export interface ToolbarState {
  tool: () => Tool;
  setTool: (tool: Tool) => void;
  snap: () => number;
  setSnap: (ticks: number) => void;
  zoom: () => number;
  setZoom: (zoom: number) => void;
}

export function TimelineToolbar(props: { state: ToolbarState }) {
  const session = useSession();
  const { clips, transport } = session.actions;
  const s = props.state;
  const [drumsOpen, setDrumsOpen] = createSignal(false);
  const [chordsOpen, setChordsOpen] = createSignal(false);
  const selected = () => clipById(session.project, session.selection.clip());
  const audio = () => (selected()?.content.type === "audio" ? selected() : undefined);
  /** "+ Track": instrument kinds from the catalog plus an audio track. */
  const trackKinds = () => [
    ...kindChoices(session.catalog()?.instruments ?? []),
    { value: AUDIO_TRACK, label: t().tracks.audioName },
  ];

  const tb = () => t().tracks.toolbar;
  const toolButton = (id: Tool, icon: JSX.Element, label: string, hint: string) => (
    <ToolButton icon={icon} label={label} title={hint} pressed={s.tool() === id} onClick={() => s.setTool(id)} />
  );
  const slider = (label: string, max: number, step: number, value: number, width: string, apply: (v: number) => void) => (
    <label class="flex items-center gap-1.5 text-muted">{label}
      <input type="range" min="0" max={max} step={step} class={`h-1 ${width}`} value={value} onChange={(e) => apply(Number(e.currentTarget.value))} /></label>
  );

  return (
    <Toolbar label={tb().label}>
      {toolButton("draw", <IconPencil />, tb().draw, tb().drawHint)}
      {toolButton("split", <IconScissors />, tb().split, tb().splitHint)}
      {toolButton("erase", <IconEraser />, tb().erase, tb().eraseHint)}
      <ToolbarDivider />
      {/* The new track's name and input pitch are decided by the `tracks.create` action. */}
      <PickSelect
        class="text-xs" placeholder={tb().addTrack} options={trackKinds()}
        onPick={(value) => void session.actions.tracks.create(value === AUDIO_TRACK ? null : value)}
      />
      <ToolButton icon={<IconDrum />} label={tb().drumPart} title={tb().drumPartHint} text onClick={() => setDrumsOpen(true)} />
      <ToolButton icon={<IconGuitar />} label={tb().chordPart} title={tb().chordPartHint} text onClick={() => setChordsOpen(true)} />
      <ToolButton icon={<IconFlag />} label={tb().marker} title={tb().markerHint} text onClick={() => void session.actions.markers.addAtPlayhead()} />
      <Show when={drumsOpen()}><DrumPartDialog onClose={() => setDrumsOpen(false)} /></Show>
      <Show when={chordsOpen()}><ChordPartDialog onClose={() => setChordsOpen(false)} /></Show>
      <ToolbarDivider />
      <label class="flex items-center gap-1.5"><span class="label">{tb().grid}</span>
        <select class="field text-xs" value={s.snap()} onChange={(e) => s.setSnap(Number(e.currentTarget.value))}>
          <For each={GRID_CHOICES}>{(o) => <option value={o.ticks}>{o.label}</option>}</For>
        </select></label>
      <Show when={selected()} keyed>
        {(clip) => <ToolButton icon={<IconLoop />} label={tb().loopClip} title={tb().loopClipHint} onClick={() => void transport.loopClip(clip)} />}
      </Show>
      <Show when={session.project.loopRegion}>
        <button class="btn btn-ghost !px-2 text-xs" onClick={() => void transport.setLoopRegion(null)}>{tb().clearLoop}</button>
      </Show>
      <ToolbarDivider />
      <label class="flex items-center gap-1.5" title={t().automation.writeHint}><span class="label">{t().automation.write}</span>
        <select class="field text-xs" value={session.tick().automationWrite} aria-label={t().automation.write}
          onChange={(e) => void session.actions.automation.setWriteMode(e.currentTarget.value as AutomationWriteMode)}>
          <For each={WRITE_MODES}>{(mode) => <option value={mode}>{t().automation.writeModes[mode]}</option>}</For>
        </select></label>
      <Show when={audio()} keyed>
        {(clip) => (
          <div class="ml-auto flex items-center gap-3 pr-1 text-xs">
            <span class="label">{tb().clip}</span>
            {slider(tb().gain, MAX_GAIN_PERCENT, 1, clip.gain * 100, "w-24", (v) => void clips.edit(clip.id, { gain: v / 100 }))}
            {slider(tb().fadeIn, clip.lengthTicks, FADE_STEP_TICKS, clip.fadeInTicks, "w-20", (v) => void clips.edit(clip.id, { fadeInTicks: v }))}
            {slider(tb().fadeOut, clip.lengthTicks, FADE_STEP_TICKS, clip.fadeOutTicks, "w-20", (v) => void clips.edit(clip.id, { fadeOutTicks: v }))}
          </div>
        )}
      </Show>
    </Toolbar>
  );
}
