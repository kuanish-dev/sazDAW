// Clip panel of a selected automation clip (instead of the piano roll): the lane name, the clip length and the pattern
// length in bars, the point tools, drawing a shape over the whole pattern, and the pattern editor itself
// (`PatternEditor`). Changes go through `actions.automation`.
import { For, type JSX, createSignal } from "solid-js";
import { BAR_TICKS, type AutomationClip, type AutomationLane, type AutomationShape } from "../../api/types";
import { targetLabel, targetRange } from "../../domain/automation";
import { MAX_CLIP_LOOP_TICKS } from "../../domain/automationCurve";
import { barLengthAt } from "../../domain/tempoMap";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { BarsStepper } from "../common/BarsStepper";
import { IconPencil, IconPointer, IconTrash } from "../common/icons";
import { ToolButton } from "../common/Toolbar";
import type { AutomationTool } from "./patternPointer";
import { PatternEditor } from "./PatternEditor";

const TOOLS: AutomationTool[] = ["select", "pencil"];
const TOOL_ICONS: Record<AutomationTool, () => JSX.Element> = { select: () => <IconPointer />, pencil: () => <IconPencil /> };
const SHAPES: AutomationShape[] = ["sine", "triangle", "saw", "square", "rampUp", "rampDown"];
/** Shape periods: 1/16 to two bars of 4/4 (labels are note fractions). */
const PERIODS: { ticks: number; label: string }[] = [
  { ticks: BAR_TICKS / 16, label: "1/16" },
  { ticks: BAR_TICKS / 8, label: "1/8" },
  { ticks: BAR_TICKS / 4, label: "1/4" },
  { ticks: BAR_TICKS / 2, label: "1/2" },
  { ticks: BAR_TICKS, label: "1/1" },
  { ticks: BAR_TICKS * 2, label: "2/1" },
];

/** `snap` is the timeline grid: pattern points snap to it. */
export function AutomationClipPanel(props: { lane: AutomationLane; clip: AutomationClip; snap: number }) {
  const session = useSession();
  const { project } = session;
  const { automation } = session.actions;
  const msg = () => t().automation;
  const [tool, setTool] = createSignal<AutomationTool>("select");
  const [shape, setShape] = createSignal<AutomationShape>("sine");
  const [period, setPeriod] = createSignal(BAR_TICKS / 4);
  const bar = () => barLengthAt(project.tempoMap, props.clip.startTick);
  const edit = (change: { lengthTicks?: number; loopTicks?: number }) => void automation.editClip(props.lane.id, props.clip.id, change);

  const applyShape = () => {
    const range = targetRange(props.lane.target, project, session.catalog());
    void automation.drawShape(props.lane.id, props.clip.id, {
      fromTick: 0, toTick: props.clip.loopTicks, shape: shape(), periodTicks: period(), min: range.min, max: range.max,
    });
  };

  return (
    <>
      <div class="flex min-h-9 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line py-1 pl-3 pr-1.5">
        <h2 class="label truncate">{targetLabel(props.lane.target, project, session.catalog())}</h2>
        <BarsStepper label={msg().clipLength} hint={msg().clipLengthHint} ticks={props.clip.lengthTicks} bar={bar()}
          max={Number.MAX_SAFE_INTEGER} onSet={(lengthTicks) => edit({ lengthTicks })} />
        <BarsStepper label={msg().patternLength} hint={msg().patternLengthHint} ticks={props.clip.loopTicks} bar={bar()}
          max={MAX_CLIP_LOOP_TICKS} onSet={(loopTicks) => edit({ loopTicks })} />
        <div class="flex shrink-0 gap-0.5">
          <For each={TOOLS}>
            {(id) => <ToolButton icon={TOOL_ICONS[id]()} label={msg().tools[id]} title={msg().toolHints[id]} pressed={tool() === id} onClick={() => setTool(id)} />}
          </For>
        </div>
        <div class="flex items-center gap-1.5">
          <select class="field text-xs" value={shape()} aria-label={msg().shape} onChange={(e) => setShape(e.currentTarget.value as AutomationShape)}>
            <For each={SHAPES}>{(s) => <option value={s}>{msg().shapes[s]}</option>}</For>
          </select>
          <select class="field w-16 text-xs" value={period()} aria-label={msg().period} title={msg().period}
            disabled={shape() === "rampUp" || shape() === "rampDown"} onChange={(e) => setPeriod(Number(e.currentTarget.value))}>
            <For each={PERIODS}>{(p) => <option value={p.ticks}>{p.label}</option>}</For>
          </select>
          <button class="btn text-xs" title={msg().applyShapeHint} onClick={applyShape}>{msg().applyShape}</button>
        </div>
        <button class="btn btn-icon ml-auto hover:!text-danger" title={msg().removeClip} aria-label={msg().removeClip}
          onClick={() => void automation.removeClip(props.lane.id, props.clip.id)}><IconTrash /></button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto">
        <PatternEditor lane={props.lane} clip={props.clip} snap={props.snap} tool={tool()} />
      </div>
    </>
  );
}
