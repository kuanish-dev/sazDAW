// Shared mouse helpers: coordinates inside an element, pointer capture, selection and zoom modifiers,
// closing popup menus on an outside click.
import { onCleanup, onMount } from "solid-js";

export interface LocalPoint {
  x: number;
  y: number;
}

/** Event point in element coordinates (from its top-left corner). */
export function localPoint(element: Element, event: { clientX: number; clientY: number }): LocalPoint {
  const rect = element.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

/** Pointer capture: moves and releases reach the element even when the mouse leaves its bounds. */
export function capture(element: Element, event: PointerEvent) {
  element.setPointerCapture(event.pointerId);
}

/** A click with Shift/Cmd/Ctrl adds to the selection instead of replacing it. */
export const isAdditive = (event: MouseEvent): boolean => event.shiftKey || event.metaKey || event.ctrlKey;

/** Ctrl/Cmd + wheel zooms (trackpad pinch arrives the same way); without a modifier the wheel scrolls. */
export const isZoomWheel = (event: WheelEvent): boolean => event.ctrlKey || event.metaKey;

/** The popup closes on a click outside it and on Esc, while the component is alive. */
export function closeOnOutside(menu: () => HTMLElement | undefined, close: () => void) {
  onMount(() => {
    const outside = (e: PointerEvent) => {
      const el = menu();
      if (el && !el.contains(e.target as Node)) close();
    };
    const onEscape = (e: KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", onEscape);
    onCleanup(() => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("keydown", onEscape);
    });
  });
}
