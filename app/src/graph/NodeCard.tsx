// Node card on the graph canvas: header (move, rename), input/output/sidechain ports and content by node kind.
import { Show, createSignal } from "solid-js";
import type { GraphNode } from "../api/types";
import { effectInfo } from "../common/catalog";
import { acceptsInput, hasOutput, nodeLabel } from "./graphRules";
import { t } from "../i18n";
import { InlineRename } from "../common/InlineRename";
import { CARD_WIDTH, HEADER_HEIGHT, cardHeight, hasSidechain, sidechainTop } from "./graphLayout";
import { NodeStrip } from "./NodeStrip";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "./role";

export type PortState = "ok" | "bad" | null;

/** Header dot color by node kind (effects get their own amber: with the blue accent "accent" equals "blue"); the signal
 *  path view marks its cards the same way. */
export const NODE_ACCENT: Readonly<Record<GraphNode["kind"]["type"], string>> = {
  instrument: "var(--color-blue)",
  audio: "var(--color-blue)",
  effect: "#d7a53a",
  bus: "var(--color-ok)",
  master: "var(--color-text)",
} as const;

export function NodeCard(props: {
  node: GraphNode;
  selected: boolean;
  /** Input port highlight while a wire is dragged. */
  portState: (sidechain: boolean) => PortState;
  onHeaderDown: (event: PointerEvent) => void;
  onOutputDown: (event: PointerEvent) => void;
  onSelect: () => void;
}) {
  const { catalog } = useProjectCopy();
  const graph = useGraph();
  const [renaming, setRenaming] = createSignal(false);
  const effect = () => (props.node.kind.type === "effect" ? props.node.kind : null);

  return (
    <div
      class="absolute select-none rounded border bg-panel shadow-md"
      classList={{ "border-focus ring-1 ring-focus": props.selected, "border-field-line": !props.selected }}
      style={{ left: `${props.node.x}px`, top: `${props.node.y}px`, width: `${CARD_WIDTH}px`, height: `${cardHeight(props.node)}px` }}
      onPointerDown={(e) => { e.stopPropagation(); props.onSelect(); }}
    >
      <div
        class="flex cursor-grab items-center gap-1.5 rounded-t border-b border-line bg-panel-2 px-2 active:cursor-grabbing"
        style={{ height: `${HEADER_HEIGHT}px` }}
        onPointerDown={props.onHeaderDown}
        onDblClick={() => props.node.kind.type !== "master" && setRenaming(true)}
        title={t().graph.node.headerHint}
      >
        <span class="h-2 w-2 shrink-0 rounded-full" style={{ background: NODE_ACCENT[props.node.kind.type] }} aria-hidden="true" />
        <InlineRename
          name={props.node.name} editing={renaming()} class="!h-5 w-full !px-1 text-xs"
          onRename={(name) => void graph.renameNode(props.node.id, name)} onClose={() => setRenaming(false)}
        >
          <span class="truncate text-xs font-semibold">{nodeLabel(props.node)}</span>
        </InlineRename>
      </div>

      <Show when={acceptsInput(props.node.kind)}>
        <Port side="left" top={HEADER_HEIGHT / 2} state={props.portState(false)} title={t().graph.node.input} />
      </Show>
      <Show when={hasSidechain(props.node)}>
        <Port side="left" top={sidechainTop(props.node)} state={props.portState(true)} title={t().graph.node.sidechain} square />
        <span class="pointer-events-none absolute bottom-[3px] left-2.5 text-[9px] text-muted">SC</span>
      </Show>
      <Show when={hasOutput(props.node.kind)}>
        <Port side="right" top={HEADER_HEIGHT / 2} state={null} title={t().graph.node.output} onDown={props.onOutputDown} />
      </Show>

      <Show when={effect()} fallback={<NodeStrip node={props.node} />}>
        {(e) => (
          <div class="flex items-center gap-2 px-3 pt-2">
            <button
              class="h-4 w-4 shrink-0 rounded-full border border-panel-3"
              classList={{ "border-ok bg-ok": !e().bypass }}
              title={e().bypass ? t().graph.node.enable : t().graph.node.bypass}
              aria-label={t().graph.node.bypass}
              onPointerDown={(ev) => ev.stopPropagation()}
              onClick={() => void graph.setEffectBypass(props.node.id, !e().bypass)}
            />
            <span class="truncate text-xs" classList={{ "text-muted line-through": e().bypass }}>{effectInfo(catalog(), e().effect)?.label ?? e().effect}</span>
          </div>
        )}
      </Show>
    </div>
  );
}

/** Port: a circle on the card edge; green/red hints while a wire is dragged. */
function Port(props: {
  side: "left" | "right";
  top: number;
  state: PortState;
  title: string;
  square?: boolean;
  onDown?: (event: PointerEvent) => void;
}) {
  return (
    <div
      class="absolute h-3 w-3 border-2 border-panel bg-panel-3"
      classList={{
        "rounded-full": !props.square,
        "rounded-sm": props.square,
        "cursor-crosshair": !!props.onDown,
        "!bg-ok": props.state === "ok",
        "!bg-danger": props.state === "bad",
      }}
      style={{ [props.side]: "-6px", top: `${props.top - 6}px` }}
      title={props.title}
      onPointerDown={props.onDown}
    />
  );
}
