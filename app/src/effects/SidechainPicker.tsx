import { For } from "solid-js";
import type { Id } from "../api/types";
import { findNode, nodeLabel, sidechainCandidates, sidechainSource } from "../graph/graphRules";
import { t } from "../i18n";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "../graph/role";

const NONE = "";

export function SidechainPicker(props: { effect: Id }) {
  const { project } = useProjectCopy();
  const graph = useGraph();
  const current = () => sidechainSource(project.graph, props.effect);
  const options = () => {
    const graph = project.graph;
    const list = sidechainCandidates(graph, props.effect);
    const key = current();
    const keyNode = key === null ? undefined : findNode(graph, key);
    return keyNode && !list.includes(keyNode) ? [keyNode, ...list] : list;
  };
  const pick = (value: string) => void graph.setSidechain(props.effect, value === NONE ? null : Number(value));
  return (
    <label class="flex items-center gap-2 text-xs" title={t().graph.effect.sidechainHint}>
      <span class="shrink-0 text-muted">{t().graph.effect.sidechain}</span>
      <select class="field min-w-0 flex-1 text-xs" value={current() ?? NONE} onChange={(e) => pick(e.currentTarget.value)}>
        <option value={NONE}>{t().graph.effect.sidechainNone}</option>
        <For each={options()}>{(node) => <option value={node.id}>{nodeLabel(node)}</option>}</For>
      </select>
    </label>
  );
}
