// "Sidechain from": picks the key source of a compressor or gate among tracks and buses; one backend call replaces the
// previous key inputs and switches the effect's key on (or off for "None").
import { For } from "solid-js";
import type { Id } from "../../api/types";
import { findNode, nodeLabel, sidechainCandidates, sidechainSource } from "../../domain/graph";
import { t } from "../../i18n";
import { useSession } from "../../state/session";

const NONE = "";

export function SidechainPicker(props: { effect: Id }) {
  const session = useSession();
  const current = () => sidechainSource(session.project.graph, props.effect);
  // The current key stays in the list even if it is not a track or bus (connected by a wire in the graph).
  const options = () => {
    const graph = session.project.graph;
    const list = sidechainCandidates(graph, props.effect);
    const key = current();
    const keyNode = key === null ? undefined : findNode(graph, key);
    return keyNode && !list.includes(keyNode) ? [keyNode, ...list] : list;
  };
  const pick = (value: string) => void session.actions.graph.setSidechain(props.effect, value === NONE ? null : Number(value));
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
