// Knob: drag up/down (Shift is finer and without snapping), wheel, arrow keys, double-click resets to default.
// While dragging, the knob snaps to the default value (and to zero); those points have ticks on the scale.
import { For, createSignal } from "solid-js";
import type { ParamInfo } from "../api/types";
import { formatParam } from "./format";
import {
  FINE_FACTOR, KEY_STEP, KNOB_END, KNOB_START, WHEEL_STEP, arcPath, dragDelta, isBipolar, positionToAngle, positionToValue,
  snapPosition, snapPositions, valueToPosition,
} from "./knobScale";
import { t } from "../i18n";
import { capture } from "./pointer";

const DEFAULT_SIZE = 40;
export function Knob(props: {
  info: Pick<ParamInfo, "label" | "min" | "max" | "default">;
  value: number;
  size?: number;
  unit?: string;
  format?: (value: number) => string;
  onChange: (value: number) => void;
}) {
  const size = () => props.size ?? DEFAULT_SIZE;
  const [dragging, setDragging] = createSignal(false);
  let start: { y: number; position: number } | null = null;

  const position = () => valueToPosition(props.value, props.info);
  const center = () => size() / 2;
  const radius = () => size() / 2 - 4;
  const angle = () => positionToAngle(position());
  const zeroAngle = () => positionToAngle(valueToPosition(0, props.info));
  const arcFrom = () => (isBipolar(props.info) ? zeroAngle() : KNOB_START);
  const text = () => (props.format ? props.format(props.value) : formatParam(props.value) + (props.unit ?? ""));

  const snaps = () => snapPositions(props.info);

  const apply = (next: number, snap = false) => {
    if (snap) next = snapPosition(next, snaps());
    const value = positionToValue(next, props.info);
    if (value !== props.value) props.onChange(value);
  };

  return (
    <div class="flex w-full flex-col items-center gap-0.5" title={t().controls.knobHint(props.info.label, text())}>
      <svg
        width={size()}
        height={size()}
        viewBox={`0 0 ${size()} ${size()}`}
        class="knob cursor-ns-resize touch-none"
        classList={{ "knob-active": dragging() }}
        role="slider"
        tabIndex={0}
        aria-label={props.info.label}
        aria-valuemin={props.info.min}
        aria-valuemax={props.info.max}
        aria-valuenow={props.value}
        onPointerDown={(e) => {
          capture(e.currentTarget, e);
          start = { y: e.clientY, position: position() };
          setDragging(true);
        }}
        onPointerMove={(e) => {
          if (!start) return;
          apply(start.position - dragDelta(e.clientY - start.y, e.shiftKey), !e.shiftKey);
        }}
        onPointerUp={() => { start = null; setDragging(false); }}
        onPointerCancel={() => { start = null; setDragging(false); }}
        onDblClick={() => props.onChange(props.info.default)}
        onWheel={(e) => { e.preventDefault(); apply(position() + (e.deltaY < 0 ? WHEEL_STEP : -WHEEL_STEP) / (e.shiftKey ? FINE_FACTOR : 1)); }}
        onKeyDown={(e) => {
          const step = e.shiftKey ? KEY_STEP / FINE_FACTOR : KEY_STEP;
          if (e.key === "ArrowUp" || e.key === "ArrowRight") apply(position() + step);
          if (e.key === "ArrowDown" || e.key === "ArrowLeft") apply(position() - step);
        }}
      >
        <path d={arcPath(center(), center(), radius(), KNOB_START, KNOB_END)} fill="none" stroke="var(--color-panel-3)" stroke-width="2.5" stroke-linecap="round" />
        <path d={arcPath(center(), center(), radius(), arcFrom(), angle())} fill="none" stroke="var(--color-accent)" stroke-width="2.5" stroke-linecap="round" />
        <For each={snaps()}>
          {(point) => (
            <line
              x1={center() + (radius() - 1) * Math.sin((positionToAngle(point) * Math.PI) / 180)}
              y1={center() - (radius() - 1) * Math.cos((positionToAngle(point) * Math.PI) / 180)}
              x2={center() + (radius() + 3) * Math.sin((positionToAngle(point) * Math.PI) / 180)}
              y2={center() - (radius() + 3) * Math.cos((positionToAngle(point) * Math.PI) / 180)}
              stroke="var(--color-muted)"
              stroke-width="1"
            />
          )}
        </For>
        <circle cx={center()} cy={center()} r={radius() - 5} fill="var(--color-panel-2)" stroke={dragging() ? "var(--color-accent)" : "var(--color-field-line)"} />
        <line
          x1={center()}
          y1={center()}
          x2={center() + (radius() - 5) * Math.sin((angle() * Math.PI) / 180)}
          y2={center() - (radius() - 5) * Math.cos((angle() * Math.PI) / 180)}
          stroke="var(--color-text)"
          stroke-width="1.5"
          stroke-linecap="round"
        />
      </svg>
      <span class="w-full truncate text-center text-[10px] leading-3 text-muted">{props.info.label.replace(/,.*$/, "")}</span>
      <span class="w-full truncate text-center font-mono text-[10px] leading-3" classList={{ "text-accent": dragging() }}>{text()}</span>
    </div>
  );
}
