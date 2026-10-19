// The horizontally visible part of wide content inside a scroller. Timeline canvases draw only this part:
// a canvas spanning the whole track on a retina screen is hundreds of megabytes of pixels, and every redraw stalls the screen.
import { createSignal, onCleanup, onMount, type Accessor } from "solid-js";

/** Window in content coordinates: starting at `left`, `width` CSS pixels wide. */
export interface ScrollWindow {
  left: number;
  width: number;
}

/** Whether the span `[x, x + w]` (content pixels) shows in the window: what is outside is not drawn. */
export const inWindow = (x: number, w: number, win: ScrollWindow): boolean => x + w >= win.left && x <= win.left + win.width;

/** Nearest ancestor with horizontal scrolling. */
function scrollerOf(el: HTMLElement): HTMLElement | null {
  return el.parentElement?.closest<HTMLElement>(".overflow-x-auto") ?? null;
}

/**
 * Tracks which part of `content` is visible in its scroller. Recomputed once per frame on scroll
 * and on resize (zoom, window). Without a scrolling ancestor the window is the size of the browser window.
 */
export function createScrollWindow(content: () => HTMLElement): Accessor<ScrollWindow> {
  const [view, setView] = createSignal<ScrollWindow>({ left: 0, width: window.innerWidth });
  let frame = 0;

  const measure = () => {
    frame = 0;
    const el = content();
    const scroller = scrollerOf(el);
    const box = el.getBoundingClientRect();
    const port = scroller?.getBoundingClientRect() ?? { left: 0, width: window.innerWidth };
    const left = Math.max(0, Math.floor(port.left - box.left));
    const width = Math.max(1, Math.ceil(Math.min(port.width, box.width - left)));
    const prev = view();
    if (prev.left !== left || prev.width !== width) setView({ left, width });
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(measure);
  };

  onMount(() => {
    const el = content();
    const scroller = scrollerOf(el);
    const resize = new ResizeObserver(schedule);
    resize.observe(el);
    if (scroller) resize.observe(scroller);
    scroller?.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    measure();
    onCleanup(() => {
      resize.disconnect();
      scroller?.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      if (frame) cancelAnimationFrame(frame);
    });
  });

  return view;
}
