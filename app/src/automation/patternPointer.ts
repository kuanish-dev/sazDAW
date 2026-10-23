import type { AutoPoint } from "../api/types";
import type { Range } from "./automationRules";
import { insertPoint } from "./automationRules";
import { curveThrough } from "./automationCurve";
import { deleteSelected, moveSelected, pointsInBox } from "./automationEdit";
import { extendStroke, startStroke, strokeOver, type PencilStroke } from "../pianoroll/controllers";
import { formatParam } from "../common/format";
import { capture, isAdditive, localPoint } from "../common/pointer";
import { HANDLE_RADIUS, POINT_RADIUS, segmentMiddle, valueAtY, yOfValue } from "./patternRender";

export type AutomationTool = "select" | "pencil";

/** How many pixels beyond a point's radius it can still be grabbed. */
const GRAB_EXTRA_PX = 3;
/** A press on empty space that moves further than this becomes a selection box instead of a new point. */
const BOX_THRESHOLD_PX = 4;

/** The selection box being dragged, in pattern pixels (any two corners). */
export interface PatternBox {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** The value next to the mouse while dragging. */
export interface PatternTip {
  x: number;
  y: number;
  text: string;
}

export interface AutomationPointerDeps {
  surface(): HTMLElement;
  tool(): AutomationTool;
  /** Pattern points in the project (without the draft). */
  points(): AutoPoint[];
  range(): Range;
  pxPerTick(): number;
  /** Tick at `x` snapped to the grid. */
  tickAt(x: number): number;
  /** Pencil point step: the grid. */
  step(): number;
  /** The pattern end: no point goes past it. */
  end(): number;
  selected(): Set<number>;
  setSelected(selected: Set<number>): void;
  setDraft(points: AutoPoint[] | null): void;
  setBox(box: PatternBox | null): void;
  setTip(tip: PatternTip | null): void;
  commit(points: AutoPoint[]): Promise<void>;
}

type Gesture =
  | { kind: "move"; base: AutoPoint[]; anchor: AutoPoint; selected: Set<number>; result: { points: AutoPoint[]; selected: Set<number> } | null }
  | { kind: "bend"; base: AutoPoint[]; index: number; result: AutoPoint[] | null }
  | { kind: "press"; x: number; y: number; additive: boolean }
  | { kind: "box"; x0: number; y0: number; additive: boolean; before: Set<number> }
  | { kind: "pencil"; stroke: PencilStroke };

export function createAutomationPointer(deps: AutomationPointerDeps) {
  let gesture: Gesture | null = null;

  const tip = (x: number, value: number, text = formatParam(value)) => deps.setTip({ x, y: yOfValue(value, deps.range()), text });

  /** Index of the point under (x, y). */
  const pointAt = (points: AutoPoint[], x: number, y: number): number | null => {
    const index = points.findIndex(
      (p) => Math.hypot(p.tick * deps.pxPerTick() - x, yOfValue(p.value, deps.range()) - y) <= POINT_RADIUS + GRAB_EXTRA_PX,
    );
    return index >= 0 ? index : null;
  };

  /** Index of the segment whose bend handle is under (x, y), or (with Alt) whose time span contains x. */
  const segmentAt = (points: AutoPoint[], x: number, y: number, anywhere: boolean): number | null => {
    for (let i = 0; i + 1 < points.length; i++) {
      const [a, b] = [points[i]!, points[i + 1]!];
      const middle = segmentMiddle(a, b, deps.range(), deps.pxPerTick());
      if (Math.hypot(middle.x - x, middle.y - y) <= HANDLE_RADIUS + GRAB_EXTRA_PX) return i;
      if (anywhere && x >= a.tick * deps.pxPerTick() && x < b.tick * deps.pxPerTick()) return i;
    }
    return null;
  };

  const pencilPoint = (x: number, y: number): AutoPoint => {
    const step = deps.step();
    return { tick: Math.min(deps.end(), Math.max(0, Math.floor(x / deps.pxPerTick() / step) * step)), value: valueAtY(y, deps.range()) };
  };

  const startPress = (points: AutoPoint[], event: PointerEvent, x: number, y: number) => {
    const hit = pointAt(points, x, y);
    if (hit !== null) {
      const selected = new Set(deps.selected());
      if (isAdditive(event)) {
        if (selected.has(hit)) selected.delete(hit);
        else selected.add(hit);
        deps.setSelected(selected);
        return;
      }
      const moving = selected.has(hit) ? selected : new Set([hit]);
      deps.setSelected(moving);
      gesture = { kind: "move", base: points, anchor: points[hit]!, selected: moving, result: null };
      return;
    }
    const segment = segmentAt(points, x, y, event.altKey);
    if (segment !== null) {
      gesture = { kind: "bend", base: points, index: segment, result: null };
      return;
    }
    gesture = { kind: "press", x, y, additive: isAdditive(event) };
  };

  return {
    onDown(event: PointerEvent) {
      const points = deps.points();
      if (event.button !== 0) return;
      capture(deps.surface(), event);
      const { x, y } = localPoint(deps.surface(), event);
      if (deps.tool() === "pencil") {
        const at = pencilPoint(x, y);
        const stroke = startStroke(at);
        gesture = { kind: "pencil", stroke };
        deps.setDraft(strokeOver(points, stroke));
        tip(x, at.value);
        return;
      }
      startPress(points, event, x, y);
    },

    onMove(event: PointerEvent) {
      const g = gesture;
      const points = deps.points();
      if (!g) return;
      const { x, y } = localPoint(deps.surface(), event);
      const range = deps.range();
      switch (g.kind) {
        case "move": {
          const dTick = deps.tickAt(x) - g.anchor.tick;
          const dValue = valueAtY(y, range) - g.anchor.value;
          g.result = moveSelected(g.base, g.selected, dTick, dValue, range);
          deps.setDraft(g.result.points);
          tip(x, Math.min(range.max, Math.max(range.min, g.anchor.value + dValue)));
          return;
        }
        case "bend": {
          const [a, b] = [g.base[g.index]!, g.base[g.index + 1]!];
          const curve = curveThrough(a.value, b.value, valueAtY(y, range));
          g.result = g.base.map((p, i) => (i === g.index ? { ...p, curve } : p));
          deps.setDraft(g.result);
          tip(x, valueAtY(y, range), `${curve > 0 ? "+" : ""}${curve.toFixed(2)}`);
          return;
        }
        case "press": {
          if (Math.hypot(x - g.x, y - g.y) < BOX_THRESHOLD_PX) return;
          gesture = { kind: "box", x0: g.x, y0: g.y, additive: g.additive, before: g.additive ? deps.selected() : new Set() };
          return;
        }
        case "box": {
          deps.setBox({ x0: g.x0, y0: g.y0, x1: x, y1: y });
          const px = deps.pxPerTick();
          const inside = pointsInBox(points, [g.x0 / px, x / px], [valueAtY(g.y0, range), valueAtY(y, range)]);
          deps.setSelected(new Set([...g.before, ...inside]));
          return;
        }
        case "pencil": {
          const at = pencilPoint(x, y);
          g.stroke = extendStroke(g.stroke, at, deps.step());
          deps.setDraft(strokeOver(points, g.stroke));
          tip(x, at.value);
          return;
        }
      }
    },

    async onUp(event: PointerEvent) {
      const g = gesture;
      const points = deps.points();
      gesture = null;
      deps.setTip(null);
      deps.setBox(null);
      if (!g) return;
      let next: AutoPoint[] | null = null;
      if (g.kind === "move" && g.result) {
        next = g.result.points;
        deps.setSelected(g.result.selected);
      } else if (g.kind === "bend") {
        next = g.result;
      } else if (g.kind === "pencil") {
        next = strokeOver(points, g.stroke);
      } else if (g.kind === "press") {
        const { x, y } = localPoint(deps.surface(), event);
        const inserted = insertPoint(points, { tick: deps.tickAt(x), value: valueAtY(y, deps.range()) });
        next = inserted.points;
        deps.setSelected(new Set([inserted.index]));
      }
      if (!next) return;
      await deps.commit(next);
      deps.setDraft(null);
    },

    /** Right click: deletes the point under the mouse, or the whole selection it belongs to. */
    onContext(event: MouseEvent) {
      event.preventDefault();
      const points = deps.points();
      const { x, y } = localPoint(deps.surface(), event);
      const hit = pointAt(points, x, y);
      if (hit === null) return;
      const selected = deps.selected();
      const doomed = selected.has(hit) ? selected : new Set([hit]);
      deps.setSelected(new Set());
      void deps.commit(deleteSelected(points, doomed));
    },

    /** The point under (x, y) — for double-click value entry. */
    pointAt: (x: number, y: number): number | null => pointAt(deps.points(), x, y),
  };
}
