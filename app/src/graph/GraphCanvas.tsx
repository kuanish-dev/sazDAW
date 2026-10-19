// Node editor canvas: background grid, connections and the dragged wire (SVG), node cards, the selected connection's slider, a hint.
// Camera and mouse come from outside (`GraphView`): the canvas only displays and forwards events.
import { For, Show } from "solid-js";
import type { GraphEdge, GraphNode, Id } from "../api/types";
import { canPreFader } from "./graphRules";
import type { ShortcutHelp } from "../common/shortcuts";
import { t } from "../i18n";
import { EdgeControls, EdgePath, WirePreview } from "./EdgePath";
import { inPort, outPort, type View } from "./graphLayout";
import type { GraphSelection } from "./graphSelection";
import { sameEdge } from "./graphSelection";
import { NodeCard } from "./NodeCard";
import type { GraphPointer, Wire } from "./pointer";
import { useGraph } from "./role";

/** Background grid dot spacing in world coordinates. */
const GRID = 24;

/** Keys deleting the selected node or connection (the canvas reacts to them, and the shortcut help is built from them). */
const DELETE_KEYS: readonly string[] = ["Delete", "Backspace"] as const;
export const GRAPH_KEYS_HELP: readonly ShortcutHelp[] = [{ keys: "Delete · ⌫", get label() { return t().graph.deleteKeyHelp; } }] as const;

export function GraphCanvas(props: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  view: View;
  wire: Wire | null;
  selection: GraphSelection;
  pointer: GraphPointer;
  ref: (el: HTMLDivElement) => void;
}) {
  const graph = useGraph();
  let container!: HTMLDivElement;
  const nodeById = (id: Id) => props.nodes.find((n) => n.id === id);
  /** A pre-fader send exists only on a regular connection from a track or bus. */
  const preFaderAllowed = (e: GraphEdge) => {
    const from = nodeById(e.from);
    return !e.sidechain && from !== undefined && canPreFader(from.kind);
  };
  const focus = () => container.focus();

  /** Connection ends in world coordinates; `null` if a node is gone. */
  const ends = (e: GraphEdge) => {
    const from = nodeById(e.from);
    const to = nodeById(e.to);
    return from && to ? { a: outPort(from), b: inPort(to, e.sidechain) } : null;
  };

  /** Where the wire goes: to the input of the node under the pointer, or just to the pointer if that node is gone. */
  const wireEnd = (w: Wire) => {
    const target = w.target;
    const node = target ? nodeById(target.id) : undefined;
    return target && node ? inPort(node, target.sidechain) : w.at;
  };

  const portState = (id: Id) => (sidechain: boolean) => {
    const target = props.wire?.target;
    if (!target || target.id !== id || target.sidechain !== sidechain) return null;
    return target.error ? ("bad" as const) : ("ok" as const);
  };

  const hint = () => {
    const w = props.wire;
    if (!w) return "";
    if (!w.target) return t().graph.wire.release;
    return w.target.error ? t().graph.wire.refused(w.target.error) : t().graph.wire.ok;
  };

  return (
    <div
      ref={(el) => { container = el; props.ref(el); }}
      class="relative flex-1 touch-none overflow-hidden outline-none"
      style={{
        "background-image": "radial-gradient(circle, var(--canvas-bar) 1px, transparent 1px)",
        "background-size": `${GRID * props.view.zoom}px ${GRID * props.view.zoom}px`,
        "background-position": `${props.view.x}px ${props.view.y}px`,
      }}
      tabIndex={0}
      data-testid="graph-canvas"
      onPointerDown={props.pointer.onBackgroundDown}
      onPointerMove={props.pointer.onMove}
      onPointerUp={props.pointer.onUp}
      onPointerCancel={props.pointer.onUp}
      onWheel={props.pointer.onWheel}
      onKeyDown={(e) => {
        if (DELETE_KEYS.includes(e.key) && !(e.target as HTMLElement).closest("input")) {
          e.preventDefault();
          props.selection.removeSelection();
        }
      }}
    >
      <div class="absolute left-0 top-0 origin-top-left" style={{ transform: `translate(${props.view.x}px, ${props.view.y}px) scale(${props.view.zoom})` }}>
        <svg class="pointer-events-none absolute left-0 top-0 overflow-visible" width="1" height="1">
          <For each={props.edges}>
            {(e) => (
              <Show when={ends(e)}>
                {(p) => {
                  const selected = () => {
                    const current = props.selection.edge();
                    return current !== null && sameEdge(current, e);
                  };
                  return <EdgePath from={p().a} to={p().b} sidechain={e.sidechain} selected={selected()} onSelect={() => { props.selection.selectEdge(e); focus(); }} />;
                }}
              </Show>
            )}
          </For>
          <Show when={props.wire}>
            {(w) => (
              <Show when={nodeById(w().from)}>
                {(from) => <WirePreview from={outPort(from())} to={wireEnd(w())} verdict={w().target ? (w().target!.error ? "bad" : "ok") : null} />}
              </Show>
            )}
          </Show>
        </svg>
        <For each={props.nodes}>
          {(node) => (
            <NodeCard
              node={node}
              selected={props.selection.nodeId() === node.id}
              portState={portState(node.id)}
              onSelect={() => { props.selection.selectNode(node.id); focus(); }}
              onHeaderDown={(e) => props.pointer.onNodeDown(e, node)}
              onOutputDown={(e) => props.pointer.onOutputDown(e, node)}
            />
          )}
        </For>
        <Show when={props.selection.edge()}>
          {(edge) => (
            <Show when={ends(edge())}>
              {(p) => (
                <EdgeControls
                  edge={edge()} from={p().a} to={p().b}
                  onLevel={(level) => void graph.connect(edge().from, edge().to, level, edge().sidechain)}
                  canPreFader={preFaderAllowed(edge())}
                  onPreFader={(preFader) => void graph.setEdgePreFader(edge().from, edge().to, preFader)}
                  onRemove={props.selection.removeSelection}
                />
              )}
            </Show>
          )}
        </Show>
      </div>
      <Show when={hint()}>
        <div class="pointer-events-none absolute bottom-3 left-3 rounded border border-field-line bg-bg px-3 py-1.5 text-xs shadow-md" classList={{ "text-danger": !!props.wire?.target?.error }} role="status">{hint()}</div>
      </Show>
    </div>
  );
}
