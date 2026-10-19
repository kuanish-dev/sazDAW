// Mixer strip routing: inserts (effects along the signal path, "+" adds one at the end), sends to buses
// (level, pre/post-fader, remove) and the main output. The strip model is `mixer/mixerStrips.ts`.
import { For, Index, Show } from "solid-js";
import type { GraphEdge, GraphNode, Id } from "../api/types";
import { kindChoices } from "../common/catalog";
import { MAX_LEVEL, findNode, nodeLabel } from "../graph/graphRules";
import { type StripModel, canBePreFader, outputTargets, sendTargets } from "./mixerStrips";
import { t } from "../i18n";
import { IconClose } from "../common/icons";
import { Knob } from "../common/Knob";
import { PickSelect } from "../common/PickSelect";
import { insertedEffectPosition } from "../graph/graphLayout";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "../graph/role";

/** New send level: half, so the bus doesn't overload right away. */
const NEW_SEND_LEVEL = 0.5;
const SEND_KNOB_SIZE = 26;
const SEND_LEVEL_RANGE = { min: 0, max: MAX_LEVEL, default: NEW_SEND_LEVEL } as const;
const percent = (level: number) => `${Math.round(level * 100)}%`;

export function InsertList(props: { model: StripModel; onOpenEffect: (node: Id, anchor: HTMLElement) => void }) {
  const { catalog, project } = useProjectCopy();
  const graph = useGraph();
  const effects = () => kindChoices(catalog()?.effects ?? []);
  /** A new effect goes at the last insert (`graph.insertEffectAt`: after it, or right before the master). */
  const add = (kind: string) => {
    const tail = findNode(project.graph, props.model.tail);
    if (!tail) return;
    const at = insertedEffectPosition(tail);
    void graph.insertEffectAt(tail.id, kind, at.x, at.y);
  };
  return (
    <div class="flex flex-col gap-0.5">
      <span class="label">{t().mixer.inserts}</span>
      <For each={props.model.inserts}>{(fx) => <InsertSlot node={fx} onOpen={props.onOpenEffect} />}</For>
      <PickSelect class="!h-6 w-full !px-1 text-[11px]" options={effects()} placeholder={t().mixer.addInsert} title={t().mixer.addInsertHint} onPick={add} />
    </div>
  );
}

function InsertSlot(props: { node: GraphNode; onOpen: (node: Id, anchor: HTMLElement) => void }) {
  const bypass = () => props.node.kind.type === "effect" && props.node.kind.bypass;
  return (
    <button
      class="flex h-6 items-center gap-1.5 truncate rounded-sm border border-field-line bg-bg px-1.5 text-left text-[11px] hover:border-focus"
      classList={{ "text-muted line-through": bypass() }}
      title={t().mixer.insertHint(nodeLabel(props.node), bypass())}
      onClick={(e) => props.onOpen(props.node.id, e.currentTarget)}
    >
      <span class="h-1.5 w-1.5 shrink-0 rounded-full" classList={{ "bg-ok": !bypass(), "bg-panel-3": bypass() }} />
      <span class="truncate">{nodeLabel(props.node)}</span>
    </button>
  );
}

export function SendList(props: { model: StripModel }) {
  const { project } = useProjectCopy();
  const graph = useGraph();
  const targets = () => sendTargets(project.graph, props.model).map((n) => ({ value: String(n.id), label: nodeLabel(n) }));
  return (
    <div class="flex flex-col gap-0.5">
      <span class="label">{t().mixer.sends}</span>
      {/* Index, not For: edges have no ids and are re-read as new objects, yet a row with a knob must survive a drag. */}
      <Index each={props.model.sends}>{(edge) => <SendRow edge={edge()} />}</Index>
      <Show when={targets().length > 0}>
        <PickSelect
          class="!h-6 w-full !px-1 text-[11px]" options={targets()} placeholder={t().mixer.addSend} title={t().mixer.addSendHint}
          onPick={(to) => void graph.connect(props.model.tail, Number(to), NEW_SEND_LEVEL, false)}
        />
      </Show>
    </div>
  );
}

function SendRow(props: { edge: GraphEdge }) {
  const { project } = useProjectCopy();
  const graph = useGraph();
  const target = () => findNode(project.graph, props.edge.to);
  const preAllowed = () => canBePreFader(project.graph, props.edge);
  return (
    <div class="flex items-center gap-1">
      <div class="w-10 shrink-0">
        <Knob
          info={{ ...SEND_LEVEL_RANGE, label: t().mixer.send }} value={props.edge.level} size={SEND_KNOB_SIZE} format={percent}
          onChange={(level) => void graph.connect(props.edge.from, props.edge.to, level, false)}
        />
      </div>
      <div class="flex min-w-0 flex-1 flex-col gap-0.5">
        <span class="truncate text-[11px]" title={target() ? nodeLabel(target()!) : ""}>{target() ? nodeLabel(target()!) : `#${props.edge.to}`}</span>
        <div class="flex gap-0.5">
          <button
            class="btn !h-4 !min-w-0 !px-1 text-[9px]" classList={{ "btn-on": props.edge.preFader }}
            disabled={!preAllowed()} aria-pressed={props.edge.preFader}
            title={preAllowed() ? t().mixer.preHint : t().mixer.postOnlyHint}
            onClick={() => void graph.setEdgePreFader(props.edge.from, props.edge.to, !props.edge.preFader)}
          >
            {props.edge.preFader ? "PRE" : "POST"}
          </button>
          <button class="btn btn-icon !h-4 !w-4 !min-w-4 hover:!text-danger" aria-label={t().mixer.removeSend} title={t().mixer.removeSend} onClick={() => void graph.disconnect(props.edge.from, props.edge.to, false)}><IconClose size={12} /></button>
        </div>
      </div>
    </div>
  );
}

export function OutputSelect(props: { model: StripModel }) {
  const { project } = useProjectCopy();
  const graph = useGraph();
  const targets = () => outputTargets(project.graph, props.model);
  return (
    <select
      class="field !h-6 w-full !px-1 text-[11px]"
      aria-label={t().mixer.output}
      title={t().mixer.outputHint}
      onChange={(e) => void graph.routeOutput(props.model.tail, Number(e.currentTarget.value))}
    >
      <Show when={!props.model.output}><option value="">{t().mixer.nowhere}</option></Show>
      {/* `selected` on the option rather than `value` on the select: a value set before the options exist is lost by the browser. */}
      <For each={targets()}>{(target) => <option value={target.id} selected={target.id === props.model.output?.to}>{target.label}</option>}</For>
    </select>
  );
}
