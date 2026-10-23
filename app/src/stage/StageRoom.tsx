import { For, Show, createMemo } from "solid-js";
import type { Id } from "../api/types";
import { clamp } from "../common/math";
import { WALL_GAP, placeAt, position, type Spot } from "./stageRules";
import { t } from "../i18n";
import { HEIGHT_STEP, KEY_STEP, type StageScene } from "./scene";
import { StageDot } from "./StageDot";
import { carriesChannel, dragDot, droppedChannel, svgPoint } from "./stagePointer";

/** How far a dot rises on screen per metre of height (a gentle bird's-eye perspective). */
const LIFT = 0.4;
/** Rings of equal distance from the listener, m: the first is where the direct sound starts to fall. */
const RINGS = [2, 4, 8, 16] as const;

export function StageRoom(props: { scene: StageScene }) {
  let svg: SVGSVGElement | undefined;
  const shape = () => props.scene.shape();
  /** The picture's size unit: dots and text keep their on-screen size whatever the room's size. */
  const unit = createMemo(() => Math.max(shape().width, shape().depth) * 0.013);
  const halfWidth = () => shape().width / 2;
  const halfDepth = () => shape().depth / 2;
  const view = createMemo(() => {
    const margin = unit() * 3;
    return { x: -halfWidth() - margin, y: -halfDepth() - margin, width: shape().width + 2 * margin, height: shape().depth + 2 * margin };
  });
  const step = createMemo(() => {
    const longest = Math.max(shape().width, shape().depth);
    return longest <= 10 ? 1 : longest <= 20 ? 2 : 5;
  });
  /** Grid lines every `step` metres from the listener out to the walls, both ways. */
  const lines = (half: number) => {
    const count = Math.floor(half / step());
    return Array.from({ length: 2 * count + 1 }, (_, i) => (i - count) * step());
  };
  const columns = createMemo(() => lines(halfWidth()));
  const rows = createMemo(() => lines(halfDepth()));
  const speakers = () => props.scene.listening() === "speakers";

  const onKey = (node: Id, event: KeyboardEvent) => {
    const steps: Record<string, (s: Spot) => Partial<Spot>> = {
      ArrowLeft: (s) => ({ pan: clamp(s.pan - KEY_STEP, -1, 1) }),
      ArrowRight: (s) => ({ pan: clamp(s.pan + KEY_STEP, -1, 1) }),
      ArrowUp: (s) => ({ depth: clamp(s.depth + KEY_STEP, -1, 1) }),
      ArrowDown: (s) => ({ depth: clamp(s.depth - KEY_STEP, -1, 1) }),
      PageUp: (s) => ({ height: clamp(s.height + HEIGHT_STEP, -1, 1) }),
      PageDown: (s) => ({ height: clamp(s.height - HEIGHT_STEP, -1, 1) }),
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
    const floor = position(shape(), spot);
    const height = spot.height;
    dragDot(event, svg, { x: floor.x, y: -floor.y }, (point) => {
      props.scene.drag(node, { ...placeAt(shape(), point.x, -point.y), height });
    }, () => props.scene.endDrag());
  };
  const onDrop = (event: DragEvent) => {
    const node = droppedChannel(event);
    if (!svg || node === null) return;
    event.preventDefault();
    const point = svgPoint(svg, event);
    props.scene.place(node, { ...placeAt(shape(), point.x, -point.y), height: 0 });
    props.scene.select(node);
  };

  return (
    <svg
      ref={svg} viewBox={`${view().x} ${view().y} ${view().width} ${view().height}`} class="block h-full w-full select-none"
      role="img" aria-label={t().stage.topView}
      onDragOver={(e) => {
        if (!carriesChannel(e)) return;
        e.preventDefault();
        if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
      }}
      onDrop={onDrop}
    >
      <defs>
        <clipPath id="stage-room-walls">
          <rect x={-halfWidth()} y={-halfDepth()} width={shape().width} height={shape().depth} />
        </clipPath>
      </defs>
      <rect x={-halfWidth()} y={-halfDepth()} width={shape().width} height={shape().depth} fill="var(--canvas-bg)" stroke="var(--canvas-bar)" stroke-width={unit() * 0.25} />
      <Show when={speakers()}>
        <rect x={-halfWidth()} y="0" width={shape().width} height={halfDepth()} fill="var(--canvas-ink-soft)" fill-opacity="0.35" />
      </Show>
      <g clip-path="url(#stage-room-walls)" stroke="var(--canvas-grid)" stroke-width={unit() * 0.1}>
        <For each={columns()}>{(x) => <line x1={x} x2={x} y1={-halfDepth()} y2={halfDepth()} />}</For>
        <For each={rows()}>{(y) => <line x1={-halfWidth()} x2={halfWidth()} y1={y} y2={y} />}</For>
      </g>
      <rect
        x={-(halfWidth() - WALL_GAP)} y={-(halfDepth() - WALL_GAP)} width={shape().width - 2 * WALL_GAP} height={shape().depth - 2 * WALL_GAP}
        fill="var(--color-accent)" fill-opacity="0.05" stroke="var(--color-accent)" stroke-opacity="0.25" stroke-width={unit() * 0.15}
        stroke-dasharray={`${unit()} ${unit()}`}
      />
      <line x1={-halfWidth()} x2={halfWidth()} y1="0" y2="0" stroke="var(--canvas-beat)" stroke-width={unit() * 0.15} stroke-dasharray={`${unit() * 1.2} ${unit() * 0.6}`} />
      <g class="pointer-events-none" font-size={String(unit() * 1.1)} fill="var(--color-muted)" text-anchor="middle">
        <text x="0" y={-halfDepth() + unit() * 1.6}>{t().stage.aheadWall}</text>
        <text x="0" y={halfDepth() - unit() * 0.8}>{speakers() ? t().stage.behindOnSpeakers : t().stage.behindWall}</text>
      </g>
      <g clip-path="url(#stage-room-walls)">
        <For each={RINGS}>
          {(radius) => (
            <>
              <circle cx="0" cy="0" r={radius} fill="none" stroke="var(--canvas-beat)" stroke-width={unit() * 0.15} stroke-dasharray={`${unit() * 0.6} ${unit() * 0.6}`} />
              <text x={unit() * 0.5} y={-radius - unit() * 0.4} font-size={String(unit() * 1.1)} fill="var(--color-muted)">{t().stage.measures(String(radius))}</text>
            </>
          )}
        </For>
      </g>
      <Listener unit={unit()} />
      <For each={props.scene.placed()}>
        {(node) => {
          const spot = () => props.scene.spotOf(node);
          const floor = () => {
            const s = spot();
            return s ? position(shape(), s) : { x: 0, y: 0, z: 0 };
          };
          return (
            <StageDot
              node={node}
              at={{ x: floor().x, y: -floor().y - floor().z * LIFT }}
              shadow={{ x: floor().x, y: -floor().y }}
              radius={unit() * (1.2 + 0.35 * (spot()?.height ?? 0))}
              fontSize={unit() * 1.4}
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

/** The listener seen from above: shoulders, ears and the nose pointing ahead (up). */
function Listener(props: { unit: number }) {
  const r = () => props.unit * 1.3;
  return (
    <g class="pointer-events-none">
      <ellipse cx="0" cy={r() * 0.35} rx={r() * 2.1} ry={r() * 0.7} fill="var(--color-panel-3)" stroke="var(--color-muted)" stroke-width={props.unit * 0.1} />
      <ellipse cx={-r()} cy="0" rx={r() * 0.3} ry={r() * 0.55} fill="var(--color-muted)" />
      <ellipse cx={r()} cy="0" rx={r() * 0.3} ry={r() * 0.55} fill="var(--color-muted)" />
      <circle cx="0" cy="0" r={r()} fill="var(--color-panel-3)" stroke="var(--color-muted)" stroke-width={props.unit * 0.15} />
      <path d={`M ${-r() * 0.35} ${-r() * 0.85} L 0 ${-r() * 1.45} L ${r() * 0.35} ${-r() * 0.85} Z`} fill="var(--color-muted)" />
      <text x="0" y={r() * 2.6} font-size={String(props.unit * 1.2)} text-anchor="middle" fill="var(--color-muted)">{t().stage.listener}</text>
    </g>
  );
}
