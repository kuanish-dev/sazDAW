// The signal path view under the graph canvas ("Signal path" in the toolbar): the way a node's sound goes, from where it
// comes from to the master (`domain/signalPath.ts`), laid out left to right like the graph. Tracks, buses and the
// master are small cards; every effect shows its live graph (`EffectGraph`), all of them hearing their nodes at once
// (`state/nodeSounds.ts` captures up to `MAX_WATCHED_NODES`). The path follows the selection, but a click on a node
// already on it keeps the whole path (selecting the bus a track goes into must not cut the path at the bus), and it stays
// while nothing is selected. A click on a card's header selects its node, so its settings open in the inspector.
import { For, Show, createMemo } from "solid-js";
import { type GraphNode, type Id, MAX_WATCHED_NODES } from "../../api/types";
import { nodeLabel } from "../../domain/graph";
import { signalPath } from "../../domain/signalPath";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { IconArrowRight } from "../common/icons";
import { EffectGraph, hasEffectGraph } from "../effects/EffectGraph";
import { NODE_ACCENT } from "./NodeCard";

export function SignalPath(props: { selected: Id | null; onSelect: (id: Id) => void }) {
  const session = useSession();
  /** The node the path is drawn through. */
  const anchor = createMemo<Id | null>((previous) => {
    const id = props.selected;
    if (id === null) return previous;
    const onPath = previous !== null && signalPath(session.project.graph, previous).some((node) => node.id === id);
    return onPath ? previous : id;
  }, null);
  const path = createMemo(() => {
    const id = anchor();
    return id === null ? [] : signalPath(session.project.graph, id);
  });
  const effects = () => path().filter((node) => node.kind.type === "effect").length;

  return (
    <section class="flex max-h-[60%] shrink-0 flex-col border-t border-line bg-panel" aria-label={t().graph.path.title}>
      <Show when={path().length > 0} fallback={<p class="p-3 text-sm text-muted">{t().graph.path.pickNode}</p>}>
        <Show when={effects() > MAX_WATCHED_NODES}>
          <p class="px-3 pt-2 text-xs text-muted">{t().graph.path.tooMany(MAX_WATCHED_NODES)}</p>
        </Show>
        <div class="flex min-h-0 items-start gap-1 overflow-auto p-2">
          <For each={path()}>
            {(node, i) => (
              <>
                <Show when={i() > 0}>
                  <IconArrowRight size={14} class="mt-1.5 shrink-0 text-muted" />
                </Show>
                <PathCard node={node} selected={node.id === props.selected} onSelect={() => props.onSelect(node.id)} />
              </>
            )}
          </For>
        </div>
      </Show>
    </section>
  );
}

/** One node of the path: its header (a click selects it) and, for an effect, its live graph. */
function PathCard(props: { node: GraphNode; selected: boolean; onSelect: () => void }) {
  const effect = () => (props.node.kind.type === "effect" ? props.node.kind : null);
  return (
    <div
      class="flex shrink-0 flex-col rounded border bg-bg"
      classList={{ "w-72": effect() !== null, "border-focus ring-1 ring-focus": props.selected, "border-line": !props.selected }}
    >
      <button class="flex min-w-0 items-center gap-1.5 rounded-t px-2 py-1 text-left text-xs hover:bg-hover" title={t().graph.path.selectHint} onClick={() => props.onSelect()}>
        <span class="h-2 w-2 shrink-0 rounded-full" style={{ background: NODE_ACCENT[props.node.kind.type] }} aria-hidden="true" />
        <span class="min-w-0 flex-1 truncate font-semibold">{nodeLabel(props.node)}</span>
        <Show when={effect()?.bypass}>
          <span class="shrink-0 text-muted">{t().graph.inspector.bypassed}</span>
        </Show>
      </button>
      <Show when={effect()}>
        {(e) => (
          <div class="px-2 pb-2">
            <Show when={hasEffectGraph(e().effect)} fallback={<p class="text-xs text-muted">{t().graph.path.noGraph}</p>}>
              <EffectGraph node={props.node} />
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
