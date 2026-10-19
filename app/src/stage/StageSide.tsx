// The room from the side: the listener's head in the middle at ear height, facing ahead (right), the floor and the
// ceiling, and every channel on the stage by how far behind or ahead (left → right) and how high (bottom → top) it
// stands. Dragging a dot sets its depth and height; its left/right stays (that is the room from above).
import { For, createMemo } from "solid-js";
import type { Id } from "../api/types";
import { clamp } from "../common/math";
import { heightAt, placeAt, position, type Spot, WALL_GAP } from "./stageRules";
import { t } from "../i18n";
import { HEIGHT_STEP, KEY_STEP, type StageScene } from "./scene";
import { StageDot } from "./StageDot";
import { dragDot } from "./stagePointer";

export function StageSide(props: { scene: StageScene }) {
  let svg: SVGSVGElement | undefined;
  const shape = () => props.scene.shape();
  const unit = createMemo(() => Math.max(shape().depth, shape().height) * 0.022);
  const halfDepth = () => shape().depth / 2;
  const half = () => shape().height / 2;
  const view = createMemo(() => {
    const margin = unit() * 2;
    return { x: -halfDepth() - margin, y: -half() - margin, width: shape().depth + 2 * margin, height: shape().height + 2 * margin };
  });

  const onKey = (node: Id, event: KeyboardEvent) => {
    const steps: Record<string, (s: Spot) => Partial<Spot>> = {
      ArrowLeft: (s) => ({ depth: clamp(s.depth - KEY_STEP, -1, 1) }),
      ArrowRight: (s) => ({ depth: clamp(s.depth + KEY_STEP, -1, 1) }),
      ArrowUp: (s) => ({ height: clamp(s.height + HEIGHT_STEP, -1, 1) }),
      ArrowDown: (s) => ({ height: clamp(s.height - HEIGHT_STEP, -1, 1) }),
    };
    const step = steps[event.key];
    if (!step) return;
    event.preventDefault();
    props.scene.nudge(node, step);
  };
  const startDrag = (node: Id, event: PointerEvent) => {
    const spot = props.scene.spotOf(node);
    if (!svg || !spot || event.button !== 0) return;
    event.preventDefault();
    props.scene.select(node);
    const at = position(shape(), spot);
    dragDot(event, svg, { x: at.y, y: -at.z }, (point) => {
      props.scene.drag(node, { pan: spot.pan, depth: placeAt(shape(), 0, point.x).depth, height: heightAt(shape(), -point.y) });
    }, () => props.scene.endDrag());
  };

  return (
    <svg ref={svg} viewBox={`${view().x} ${view().y} ${view().width} ${view().height}`} class="block h-auto w-full select-none" role="img" aria-label={t().stage.sideView}>
      <rect x={-halfDepth()} y={-half()} width={shape().depth} height={shape().height} fill="var(--canvas-bg)" stroke="var(--canvas-bar)" stroke-width={unit() * 0.2} />
      {/* Ear height: a dot on this line sounds level with the listener; above or below it takes the height cue. */}
      <line x1={-halfDepth()} x2={halfDepth()} y1="0" y2="0" stroke="var(--canvas-beat)" stroke-width={unit() * 0.12} stroke-dasharray={`${unit() * 0.6} ${unit() * 0.6}`} />
      <rect
        x={-(halfDepth() - WALL_GAP)} y={-(half() - WALL_GAP)} width={shape().depth - 2 * WALL_GAP} height={shape().height - 2 * WALL_GAP}
        fill="var(--color-accent)" fill-opacity="0.05"
      />
      <g class="pointer-events-none">
        <circle cx="0" cy="0" r={unit() * 1.1} fill="var(--color-panel-3)" stroke="var(--color-muted)" stroke-width={unit() * 0.15} />
        <path d={`M ${unit() * 0.9} ${-unit() * 0.4} L ${unit() * 1.6} 0 L ${unit() * 0.9} ${unit() * 0.4} Z`} fill="var(--color-muted)" />
        <line x1="0" x2="0" y1={unit() * 1.1} y2={half()} stroke="var(--color-muted)" stroke-width={unit() * 0.25} />
      </g>
      <For each={props.scene.placed()}>
        {(node) => {
          const at = () => {
            const spot = props.scene.spotOf(node);
            return spot ? position(shape(), spot) : { x: 0, y: 0, z: 0 };
          };
          return (
            <StageDot
              node={node}
              at={{ x: at().y, y: -at().z }}
              radius={unit() * 0.9}
              fontSize={unit() * 1.2}
              color={props.scene.colorOf(node)}
              name={props.scene.nameOf(node)}
              selected={props.scene.selected() === node}
              onPointerDown={(e) => startDrag(node, e)}
              onWheel={(e) => {
                e.preventDefault();
                props.scene.nudge(node, (s) => ({ height: clamp(s.height + (e.deltaY < 0 ? HEIGHT_STEP : -HEIGHT_STEP), -1, 1) }));
              }}
              onDblClick={() => props.scene.nudge(node, () => ({ height: 0 }))}
              onKeyDown={(e) => onKey(node, e)}
            />
          );
        }}
      </For>
    </svg>
  );
}
