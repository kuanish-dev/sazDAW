// MIDI controller lane under velocity (collapsible so it does not take space from the note grid): lane choice (pitch bend,
// mod wheel, sustain, expression, aftertouch, "Other CC…"; lanes with points are marked with a dot) and the lane canvas. Assembly only:
// drawing — `controllerRender`, mouse — `controllerPointer`, editing — `session.actions.notes.setControllerLane` (one edit per gesture).
import { For, Show, createEffect, createSignal } from "solid-js";
import type { ControllerKind, ControllerPoint } from "../../api/types";
import { MAX_CC_NUMBER, controllerFromKey, controllerKey, controllerLabel, laneChoices, lanePoints, lanesOf, sameKind } from "../../domain/controllers";
import type { NoteClip } from "../../domain/noteClip";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { IconChevronDown, IconChevronRight } from "../common/icons";
import { NumberField } from "../common/NumberField";
import { createControllerPointer } from "./controllerPointer";
import { drawController } from "./controllerRender";

const OTHER_CC = "other";

export interface ControllerLaneProps {
  clip: NoteClip;
  width: number;
  pxPerTick: number;
  viewLength: number;
  lineStep: number;
  barTicks: number;
  beatTicks: number;
  /** Point step while drawing (the panel grid; without snapping — a fine step). */
  step: number;
  /** Horizontal scroll of the note grid and its scrollbar width: the columns must line up. */
  scrollLeft: number;
  gutter: number;
}

export function ControllerLaneView(props: ControllerLaneProps) {
  const notes = useSession().actions.notes;
  const [kind, setKind] = createSignal<ControllerKind>({ type: "pitchBend" });
  /** A custom CC number from "Other CC…" while it has no points (otherwise it is in the list anyway). */
  const [extra, setExtra] = createSignal<ControllerKind | null>(null);
  const [askNumber, setAskNumber] = createSignal(false);
  const [ccNumber, setCcNumber] = createSignal(2);
  const [draft, setDraft] = createSignal<ControllerPoint[] | null>(null);
  const [expanded, setExpanded] = createSignal(false);
  let canvas!: HTMLCanvasElement;
  let host!: HTMLDivElement;

  const saved = () => lanePoints(props.clip.content, kind());
  const shown = () => draft() ?? saved();
  const hasData = (k: ControllerKind) => lanesOf(props.clip.content).some((lane) => sameKind(lane.kind, k));

  const pointer = createControllerPointer({
    canvas: () => canvas, kind, points: saved, pxPerTick: () => props.pxPerTick, step: () => props.step,
    endTick: () => props.viewLength, setDraft,
    // The action shows the lane at once, so the draft is dropped after it without the lane flashing its old shape.
    commit: (points) => notes.setControllerLane(props.clip.id, kind(), points),
  });

  const choose = (key: string) => {
    if (key === OTHER_CC) return setAskNumber(true);
    const next = controllerFromKey(key);
    if (next) setKind(next);
  };
  const chooseCc = (number: number) => {
    const next: ControllerKind = { type: "cc", number };
    setExtra(next);
    setKind(next);
    setAskNumber(false);
  };

  createEffect(() => {
    if (expanded()) host.scrollLeft = props.scrollLeft;
  });
  createEffect(() => expanded() && drawController(canvas, {
    width: props.width, pxPerTick: props.pxPerTick, viewLength: props.viewLength, contentTicks: props.clip.content.loopTicks,
    lineStep: props.lineStep, barTicks: props.barTicks, beatTicks: props.beatTicks, kind: kind(), points: shown(),
  }));

  return (
    <div class="shrink-0 border-t border-line">
      <div class="flex min-h-8 flex-wrap items-center gap-2 py-1 pl-1.5 pr-1.5 text-xs">
        <button
          class="label flex h-6 items-center gap-1 rounded px-1 hover:bg-hover hover:text-text" aria-expanded={expanded()} onClick={() => setExpanded(!expanded())}
          title={expanded() ? t().pianoRoll.collapseLane : t().pianoRoll.expandLane}
        >
          {expanded() ? <IconChevronDown size={14} /> : <IconChevronRight size={14} />}{t().pianoRoll.controller}
          <Show when={lanesOf(props.clip.content).length}><span class="h-1.5 w-1.5 rounded-full bg-accent" /></Show>
        </button>
        <select class="field !h-6 text-xs" aria-label={t().pianoRoll.controller} value={controllerKey(kind())} onChange={(e) => {
          choose(e.currentTarget.value);
          // "Other CC…" is not a lane: the list keeps showing the current one until a number is chosen.
          e.currentTarget.value = controllerKey(kind());
        }} title={t().pianoRoll.laneHint}>
          <For each={laneChoices(props.clip.content, extra())}>
            {(k) => <option value={controllerKey(k)}>{`${hasData(k) ? "● " : ""}${controllerLabel(k)}`}</option>}
          </For>
          <option value={OTHER_CC}>{t().pianoRoll.otherCc}</option>
        </select>
        <Show when={askNumber()}>
          <span class="text-muted">{t().pianoRoll.ccNumber}</span>
          <NumberField value={ccNumber()} min={0} max={MAX_CC_NUMBER} label={t().pianoRoll.ccNumber} class="w-14" onCommit={setCcNumber} />
          <button class="btn !h-6 text-xs" onClick={() => chooseCc(ccNumber())}>{t().pianoRoll.select}</button>
          <button class="btn !h-6 text-xs" onClick={() => setAskNumber(false)}>{t().common.cancel}</button>
        </Show>
        <Show when={expanded()}><span class="text-muted">{t().pianoRoll.drawHint}</span></Show>
        <button
          class="btn btn-ghost ml-auto !h-6 !px-2 text-xs" disabled={saved().length === 0}
          onClick={() => void notes.setControllerLane(props.clip.id, kind(), [])} title={t().pianoRoll.clearLaneHint}
        >{t().pianoRoll.clearLane}</button>
      </div>
      <Show when={expanded()}>
        <div ref={host} class="overflow-hidden" style={{ "margin-right": `${props.gutter}px` }}>
          <canvas
            ref={canvas} class="block touch-none"
            onPointerDown={pointer.onDown} onPointerMove={pointer.onMove} onPointerUp={() => void pointer.onUp()}
            onContextMenu={(e) => e.preventDefault()}
          />
        </div>
      </Show>
    </div>
  );
}
