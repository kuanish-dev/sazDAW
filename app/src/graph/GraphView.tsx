import { Show, createMemo, createSignal } from "solid-js";
import type { Id } from "../api/types";
import { GraphCanvas } from "./GraphCanvas";
import { CARD_WIDTH, autoLayout, insertedEffectPosition, toWorld, type Point, type View } from "./graphLayout";
import { createGraphSelection } from "./graphSelection";
import { GraphToolbar } from "./GraphToolbar";
import { Inspector } from "./Inspector";
import { createGraphPointer, type Wire } from "./pointer";
import { SignalPath } from "./SignalPath";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "./role";

const HOME: Readonly<View> = { x: 0, y: 0, zoom: 1 } as const;
/** A new node is placed slightly above the middle of the visible area. */
const NEW_NODE_LIFT = 30;

export function GraphView() {
  const { catalog, project } = useProjectCopy();
  const graph = useGraph();

  const [view, setView] = createSignal<View>(HOME);
  const [drafts, setDrafts] = createSignal<Record<Id, Point>>({});
  const [wire, setWire] = createSignal<Wire | null>(null);
  const [pathOpen, setPathOpen] = createSignal(false);
  let container!: HTMLDivElement;

  /** Nodes with dragged positions (drafts) applied; the rest stay the same objects, so they don't re-render. */
  const nodes = createMemo(() => project.graph.nodes.map((n) => (drafts()[n.id] ? { ...n, ...drafts()[n.id] } : n)));
  const selection = createGraphSelection({ graph, nodes, edges: () => project.graph.edges });
  const selectAndFocus = (id: Id | null) => {
    selection.selectNode(id);
    container.focus();
  };

  const pointer = createGraphPointer({
    graph,
    data: () => project.graph,
    nodes,
    container: () => container,
    view,
    setView,
    setDrafts,
    setWire,
    select: selectAndFocus,
  });

  /** Middle of the visible area in world coordinates (where new nodes go). */
  const visibleCentre = (): Point => {
    const c = toWorld(view(), { x: container.clientWidth / 2, y: container.clientHeight / 2 });
    return { x: Math.round(c.x - CARD_WIDTH / 2), y: Math.round(c.y - NEW_NODE_LIFT) };
  };

  const selectNew = (id: Id | undefined) => {
    if (id !== undefined) selection.selectNode(id);
  };

  /** Inserts an effect at the selected node without breaking the path (`graph.insertEffectAt` decides after or before it). */
  const insertEffect = async (kind: string) => {
    const n = selection.node();
    if (!n) return;
    const at = insertedEffectPosition(n);
    selectNew(await graph.insertEffectAt(n.id, kind, at.x, at.y));
  };

  const removeKeepPath = () => {
    const n = selection.node();
    if (n?.kind.type !== "effect") return;
    void graph.removeEffectKeepPath(n.id);
    selection.selectNode(null);
  };

  const arrange = async () => {
    await graph.arrange(autoLayout(nodes(), project.graph.edges, project.tracks.map((t) => t.id)));
    setView(HOME);
  };

  return (
    <div class="flex h-full min-h-[420px] flex-col">
      <GraphToolbar
        effects={catalog()?.effects ?? []}
        canRemove={selection.canRemove()}
        canInsert={selection.node() !== null}
        canKeepPath={selection.node()?.kind.type === "effect"}
        zoom={view().zoom}
        pathOpen={pathOpen()}
        onAddEffect={async (kind) => { const at = visibleCentre(); selectNew(await graph.addEffect(kind, at.x, at.y)); }}
        onInsertEffect={(kind) => void insertEffect(kind)}
        onAddBus={async () => { const at = visibleCentre(); selectNew(await graph.addBus(at.x, at.y)); }}
        onRemove={selection.removeSelection}
        onRemoveKeepPath={removeKeepPath}
        onArrange={() => void arrange()}
        onResetView={() => setView(HOME)}
        onTogglePath={() => setPathOpen(!pathOpen())}
      />
      <div class="flex min-h-0 flex-1">
        <div class="flex min-w-0 flex-1 flex-col">
          <GraphCanvas
            ref={(el) => { container = el; }}
            nodes={nodes()} edges={project.graph.edges} view={view()} wire={wire()} selection={selection} pointer={pointer}
          />
          <Show when={pathOpen()}>
            <SignalPath selected={selection.node()?.id ?? null} onSelect={selectAndFocus} />
          </Show>
        </div>
        <Inspector node={selection.node()} onInsertEffect={(kind) => void insertEffect(kind)} />
      </div>
    </div>
  );
}
