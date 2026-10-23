import { Show } from "solid-js";
import type { Id } from "../api/types";
import { peakToMeter } from "../mixer/mixer";
import { t } from "../i18n";
import { useMixer } from "../mixer/role";

export interface DotPoint {
  x: number;
  y: number;
}

export function StageDot(props: {
  node: Id;
  at: DotPoint;
  /** Where its shadow falls (the room from above lifts a high dot above its place on the floor). */
  shadow?: DotPoint;
  radius: number;
  fontSize: number;
  color: string;
  name: string;
  selected: boolean;
  onPointerDown: (event: PointerEvent) => void;
  onWheel: (event: WheelEvent) => void;
  onDblClick: () => void;
  onKeyDown: (event: KeyboardEvent) => void;
}) {
  const mixer = useMixer();
  /** 0…1 on the meter's scale. */
  const loudness = () => {
    const level = mixer.levels[props.node];
    return level ? peakToMeter(Math.max(level.l.level, level.r.level)) : 0;
  };
  return (
    <g
      class="cursor-grab outline-none active:cursor-grabbing" tabindex="0" role="button" aria-label={props.name} aria-pressed={props.selected}
      onPointerDown={(e) => props.onPointerDown(e)} onWheel={(e) => props.onWheel(e)} onDblClick={() => props.onDblClick()}
      onKeyDown={(e) => props.onKeyDown(e)}
    >
      <title>{t().stage.dotHint(props.name)}</title>
      <Show when={props.shadow}>
        {(shadow) => (
          <>
            <ellipse cx={shadow().x} cy={shadow().y} rx={props.radius * 0.9} ry={props.radius * 0.45} fill="var(--canvas-ink-soft)" />
            <line x1={shadow().x} y1={shadow().y} x2={props.at.x} y2={props.at.y} stroke="var(--canvas-ink-soft)" stroke-width={props.radius * 0.12} />
          </>
        )}
      </Show>
      <circle cx={props.at.x} cy={props.at.y} r={props.radius * (1.3 + 1.7 * loudness())} fill={props.color} opacity={0.08 + 0.42 * loudness()} />
      <circle
        cx={props.at.x} cy={props.at.y} r={props.radius} fill={props.color}
        stroke={props.selected ? "var(--canvas-ink)" : "var(--canvas-bg-deep)"} stroke-width={props.radius * (props.selected ? 0.3 : 0.15)}
      />
      <text
        x={props.at.x + props.radius * 1.6} y={props.at.y + props.fontSize * 0.35} font-size={String(props.fontSize)}
        fill={props.selected ? "var(--canvas-ink)" : "var(--color-text)"} class="pointer-events-none select-none"
      >
        {props.name}
      </text>
    </g>
  );
}
