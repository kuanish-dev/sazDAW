import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import type { AutoPoint, AutomationClip, AutomationLane } from "../api/types";
import { pointSnap, targetRange } from "./automationRules";
import { deleteSelected } from "./automationEdit";
import { formatParam } from "../common/format";
import { clamp } from "../common/math";
import { playheadInLoop } from "../pianoroll/noteClip";
import type { ShortcutHelp } from "../common/shortcuts";
import { snapNearestInBars } from "../common/tempoMap";
import { t, type Messages } from "../i18n";
import { createAutomationPointer, type AutomationTool, type PatternBox, type PatternTip } from "./patternPointer";
import { AUTOMATION_HEIGHT, drawAutomation, yOfValue } from "./patternRender";
import { usePlayback, useProjectCopy } from "../core/roles";
import { useAutomation } from "./role";

type PatternKeyAction = keyof Messages["automation"]["keys"];

interface PatternKey {
  keys: string;
  action: PatternKeyAction;
  matches(event: KeyboardEvent, meta: boolean): boolean;
}

/** Pattern editor keys: they work by this table, and the help is built from it too. */
const PATTERN_KEYS: readonly PatternKey[] = [
  { keys: "Delete · ⌫", action: "remove", matches: (e) => e.key === "Delete" || e.key === "Backspace" },
  { keys: "⌘C", action: "copy", matches: (e, meta) => meta && e.code === "KeyC" },
  { keys: "⌘V", action: "paste", matches: (e, meta) => meta && e.code === "KeyV" },
  { keys: "⌘A", action: "selectAll", matches: (e, meta) => meta && e.code === "KeyA" },
  { keys: "Esc", action: "deselect", matches: (e) => e.key === "Escape" },
] as const;

/** `label` is a getter so the help follows the interface language. */
export const PATTERN_KEYS_HELP: readonly ShortcutHelp[] = PATTERN_KEYS.map(({ keys, action }) => ({
  keys,
  get label() {
    return t().automation.keys[action];
  },
}));

export function PatternEditor(props: { lane: AutomationLane; clip: AutomationClip; snap: number; tool: AutomationTool }) {
  const { playhead } = usePlayback();
  const { catalog, project } = useProjectCopy();
  const automation = useAutomation();
  let canvas!: HTMLCanvasElement;
  let surface!: HTMLDivElement;
  const [width, setWidth] = createSignal(1);
  onMount(() => {
    const observer = new ResizeObserver(() => setWidth(Math.max(1, surface.clientWidth)));
    observer.observe(surface);
    onCleanup(() => observer.disconnect());
  });
  const loop = () => props.clip.loopTicks;
  const pxPerTick = () => width() / loop();
  /** Points during a gesture (until the mouse is released). */
  const [draft, setDraft] = createSignal<AutoPoint[] | null>(null);
  const [selected, setSelected] = createSignal<Set<number>>(new Set());
  const [box, setBox] = createSignal<PatternBox | null>(null);
  const [tip, setTip] = createSignal<PatternTip | null>(null);
  /** The point whose value is being typed. */
  const [editing, setEditing] = createSignal<number | null>(null);
  const range = createMemo(() => targetRange(props.lane.target, project, catalog()));
  const lanePoints = () => props.clip.points;
  const points = () => draft() ?? lanePoints();
  /** Pattern tick at `x`: snapped to the song grid at the clip's place, inside the pattern. */
  const tickAt = (x: number) => {
    const start = props.clip.startTick;
    return clamp(snapNearestInBars(project.tempoMap, start + x / pxPerTick(), pointSnap(props.snap)) - start, 0, loop());
  };
  /** The playhead inside the pattern: where in the repeat the song is, while it is in the clip. */
  const localPlayhead = (): number | null => {
    const at = playhead();
    return at === null ? null : playheadInLoop(props.clip.startTick, props.clip.lengthTicks, loop(), at);
  };

  createEffect(on(() => props.clip.id, () => setSelected(new Set<number>())));

  createEffect(() => {
    drawAutomation(canvas, {
      width: width(), pxPerTick: pxPerTick(), tempoMap: project.tempoMap, startTick: props.clip.startTick, points: points(), range: range(),
      selected: selected(), box: box(), playhead: localPlayhead(),
    });
  });

  /** Points go back sorted and inside the pattern (a drag may carry some past its end). */
  const commit = async (list: AutoPoint[]) => {
    const inside = list.map((p) => ({ ...p, tick: clamp(p.tick, 0, loop()) })).sort((a, b) => a.tick - b.tick);
    await automation.setPoints(props.lane.id, props.clip.id, inside);
  };

  const pointer = createAutomationPointer({
    surface: () => surface,
    tool: () => props.tool,
    points: lanePoints,
    range,
    pxPerTick,
    tickAt,
    step: () => pointSnap(props.snap),
    end: loop,
    selected,
    setSelected,
    setDraft,
    setBox,
    setTip,
    commit,
  });

  /** Each runner says whether it acted: a key with nothing to do (Delete without a selection) is left to the window. */
  const runners: Record<PatternKeyAction, (list: AutoPoint[]) => boolean> = {
    remove(list) {
      if (selected().size === 0) return false;
      void commit(deleteSelected(list, selected()));
      setSelected(new Set<number>());
      return true;
    },
    copy(list) {
      automation.copyPoints(list, selected());
      return true;
    },
    paste(list) {
      const pasted = automation.pastedPoints(list, localPlayhead() ?? 0);
      if (!pasted) return false;
      setSelected(pasted.selected);
      void commit(pasted.points);
      return true;
    },
    selectAll(list) {
      setSelected(new Set(list.map((_, i) => i)));
      return true;
    },
    deselect() {
      setSelected(new Set<number>());
      return true;
    },
  };

  const onKey = (event: KeyboardEvent) => {
    const key = PATTERN_KEYS.find((k) => k.matches(event, event.metaKey || event.ctrlKey));
    if (key && runners[key.action](lanePoints())) event.preventDefault();
  };

  const onDoubleClick = (event: MouseEvent) => {
    const rect = surface.getBoundingClientRect();
    const hit = pointer.pointAt(event.clientX - rect.left, event.clientY - rect.top);
    if (hit !== null) setEditing(hit);
  };

  const editedPoint = () => {
    const index = editing();
    return index === null ? undefined : lanePoints()[index];
  };

  const finishEdit = (text: string | null) => {
    const index = editing();
    setEditing(null);
    const list = lanePoints();
    const value = text === null ? Number.NaN : Number(text.replace(",", "."));
    if (index === null || !list[index] || !Number.isFinite(value)) return;
    const r = range();
    const clamped = clamp(r.whole ? Math.round(value) : value, r.min, r.max);
    void commit(list.map((p, i) => (i === index ? { ...p, value: clamped } : p)));
  };

  return (
    <div ref={surface} class="relative touch-none outline-none" tabindex="0" data-owns-keys
      style={{ height: `${AUTOMATION_HEIGHT}px`, cursor: props.tool === "pencil" ? "cell" : "crosshair" }}
      onPointerDown={(e) => {
        surface.focus({ preventScroll: true });
        pointer.onDown(e);
      }}
      onPointerMove={(e) => pointer.onMove(e)} onPointerUp={(e) => void pointer.onUp(e)}
      onContextMenu={(e) => pointer.onContext(e)} onDblClick={onDoubleClick} onKeyDown={onKey}>
      <canvas ref={canvas} class="absolute left-0 top-0 block" />
      <Show when={tip()}>
        {(t) => (
          <div class="pointer-events-none absolute z-10 rounded border border-field-line bg-bg px-1.5 py-0.5 text-[11px] tabular-nums shadow-md"
            style={{ left: `${t().x + 10}px`, top: `${Math.max(0, t().y - 24)}px` }}>{t().text}</div>
        )}
      </Show>
      <Show when={editedPoint()}>
        {(p) => (
          <input class="field absolute z-10 !h-6 w-20 text-xs" type="text" inputmode="decimal" value={formatParam(p().value)}
            style={{ left: `${Math.min(p().tick * pxPerTick() + 8, width() - 88)}px`, top: `${Math.max(0, yOfValue(p().value, range()) - 12)}px` }}
            ref={(el) => queueMicrotask(() => el.select())}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") finishEdit(e.currentTarget.value);
              if (e.key === "Escape") finishEdit(null);
            }}
            onBlur={(e) => editing() !== null && finishEdit(e.currentTarget.value)}
            onPointerDown={(e) => e.stopPropagation()} />
        )}
      </Show>
    </div>
  );
}
