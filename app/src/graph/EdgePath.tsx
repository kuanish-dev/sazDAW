import { Show } from "solid-js";
import type { GraphEdge } from "../api/types";
import { MAX_LEVEL } from "./graphRules";
import { t } from "../i18n";
import { IconClose } from "../common/icons";
import { bezierPath, bezierPoint, type Point } from "./graphLayout";

/** A curve between ports; sidechain is dashed. A wide invisible stroke makes the connection easy to click. */
export function EdgePath(props: { from: Point; to: Point; sidechain: boolean; selected: boolean; onSelect: () => void }) {
  const d = () => bezierPath(props.from, props.to);
  return (
    <g class="pointer-events-auto cursor-pointer" onPointerDown={(e) => { e.stopPropagation(); props.onSelect(); }}>
      <path d={d()} fill="none" stroke="transparent" stroke-width="14" />
      <path
        d={d()}
        fill="none"
        stroke={props.selected ? "var(--color-accent)" : "var(--color-muted)"}
        stroke-width={props.selected ? 3 : 2}
        stroke-dasharray={props.sidechain ? "6 5" : undefined}
      />
    </g>
  );
}

/** The wire the user drags: green/red over an input, neutral otherwise. */
export function WirePreview(props: { from: Point; to: Point; verdict: "ok" | "bad" | null }) {
  const color = () => (props.verdict === "ok" ? "var(--color-ok)" : props.verdict === "bad" ? "var(--color-danger)" : "var(--color-accent)");
  return <path d={bezierPath(props.from, props.to)} fill="none" stroke={color()} stroke-width="2.5" stroke-dasharray="4 4" />;
}

/** Level slider, pre/post-fader toggle (only for a regular connection from a track or bus) and a remove button
 * at the middle of the selected connection. */
export function EdgeControls(props: {
  edge: GraphEdge;
  from: Point;
  to: Point;
  canPreFader: boolean;
  onLevel: (level: number) => void;
  onPreFader: (preFader: boolean) => void;
  onRemove: () => void;
}) {
  const mid = () => bezierPoint(props.from, props.to, 0.5);
  return (
      <div
        class="absolute flex -translate-x-1/2 -translate-y-1/2 items-center gap-1 rounded border border-field-line bg-bg py-0.5 pl-2 pr-0.5 shadow-md"
        style={{ left: `${mid().x}px`, top: `${mid().y}px` }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <input
          type="range"
          min="0"
          max={MAX_LEVEL * 100}
          class="h-1 w-20"
          value={props.edge.level * 100}
          aria-label={t().graph.edge.level}
          title={t().graph.edge.level}
          onInput={(e) => props.onLevel(Number(e.currentTarget.value) / 100)}
        />
        <span class="w-9 text-right font-mono text-[10px]">{Math.round(props.edge.level * 100)}%</span>
        <Show when={props.canPreFader}>
          <button
            class="btn !h-5 !px-1.5 text-[10px]"
            classList={{ "btn-on": props.edge.preFader }}
            aria-pressed={props.edge.preFader}
            title={t().graph.edge.preFaderHint}
            onClick={() => props.onPreFader(!props.edge.preFader)}
          >
            {props.edge.preFader ? t().graph.edge.preFader : t().graph.edge.postFader}
          </button>
        </Show>
        <button class="btn btn-icon !h-5 !w-5 !min-w-5 hover:!text-danger" aria-label={t().graph.edge.remove} title={t().graph.edge.removeHint} onClick={props.onRemove}><IconClose size={12} /></button>
      </div>
  );
}
