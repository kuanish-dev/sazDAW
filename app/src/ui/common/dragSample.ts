// Dragging a sound from the sound browser onto the timeline (and onto "Project") with the mouse, using pointer events.
// Not HTML5 drag&drop: in a Tauri window on macOS (WebKit) it does not start on `user-select: none` and depends on
// the window itself intercepting drags; track reordering (`trackReorder`) already works on pointer events.
import { createSignal, onCleanup } from "solid-js";
import type { SampleSource } from "../../domain/dropSample";

/** What is being dragged and where the cursor is (for the cursor label and the clip length "ghost"). */
export interface SampleDrag {
  source: SampleSource;
  label: string;
  /** Clip length in ticks for the "ghost", if known. */
  lengthTicks: number | null;
  x: number;
  y: number;
}

/** Drop target: what to do while the cursor is over it, on leave and on release. Screen coordinates. */
export interface SampleDropTarget {
  over?(x: number, y: number): void;
  leave?(): void;
  drop(source: SampleSource, x: number, y: number): void;
}

/** Mouse movement (pixels) after which a press becomes a drag rather than a click. */
const START_DISTANCE = 5;

const [current, setCurrent] = createSignal<SampleDrag | null>(null);
export const draggedSample = current;

const targets = new Map<Element, SampleDropTarget>();

/** Makes the element a sound drop target while the component is alive. */
export function registerSampleDrop(el: HTMLElement, target: SampleDropTarget) {
  targets.set(el, target);
  onCleanup(() => targets.delete(el));
}

const targetAt = (x: number, y: number): { el: Element; target: SampleDropTarget } | null => {
  for (let el = document.elementFromPoint(x, y); el; el = el.parentElement) {
    const target = targets.get(el);
    if (target) return { el, target };
  }
  return null;
};

/**
 * Press on a source (a sound row): if the mouse moves further than `START_DISTANCE`, a drag starts;
 * without movement nothing happens and a normal click works as before. Release outside a target or Esc cancels.
 */
export function startSampleDrag(event: PointerEvent, source: SampleSource, label: string, lengthTicks: number | null = null) {
  if (event.button !== 0) return;
  const origin = { x: event.clientX, y: event.clientY };
  let over: { el: Element; target: SampleDropTarget } | null = null;

  const setOver = (next: typeof over) => {
    if (over?.el === next?.el) return;
    over?.target.leave?.();
    over = next;
  };

  const finish = () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", cancel);
    window.removeEventListener("keydown", onKey, true);
    document.body.style.cursor = "";
    setOver(null);
    setCurrent(null);
  };

  const onMove = (e: PointerEvent) => {
    if (!current()) {
      if (Math.hypot(e.clientX - origin.x, e.clientY - origin.y) < START_DISTANCE) return;
      document.body.style.cursor = "copy";
    }
    setCurrent({ source, label, lengthTicks, x: e.clientX, y: e.clientY });
    const hit = targetAt(e.clientX, e.clientY);
    setOver(hit);
    hit?.target.over?.(e.clientX, e.clientY);
  };

  const onUp = (e: PointerEvent) => {
    if (!current()) return finish();
    const hit = targetAt(e.clientX, e.clientY);
    finish();
    // A click after the drag would act as "listen": swallow it once.
    const swallow = (click: Event) => click.stopPropagation();
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    hit?.target.drop(source, e.clientX, e.clientY);
  };

  const cancel = () => finish();
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.stopPropagation();
    finish();
  };

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("keydown", onKey, true);
}
