// Theme colors for drawing code (canvas cannot see CSS classes). `settings/appearance.ts` sets the theme and accent as attributes
// on the page root; here we only notice when they change so canvases redraw.
import { createSignal } from "solid-js";

const [look, setLook] = createSignal(0);
// Whoever changes `data-theme`/`data-accent`, canvases reading `palette()` will redraw.
if (typeof MutationObserver === "function") {
  new MutationObserver(() => setLook((n) => n + 1))
    .observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-accent"] });
}

/** Colors needed by drawing code. Read inside rendering so it redraws when the theme or accent changes. */
export function palette() {
  look();
  const style = getComputedStyle(document.documentElement);
  const get = (name: string) => style.getPropertyValue(name).trim();
  return {
    bg: get("--canvas-bg"),
    bgDeep: get("--canvas-bg-deep"),
    rowA: get("--canvas-row-a"),
    rowB: get("--canvas-row-b"),
    bar: get("--canvas-bar"),
    beat: get("--canvas-beat"),
    grid: get("--canvas-grid"),
    text: get("--color-muted"),
    ink: get("--canvas-ink"),
    inkSoft: get("--canvas-ink-soft"),
    dim: get("--canvas-dim"),
    keyBlack: get("--key-black"),
    keyWhite: get("--key-white"),
    noteSelected: get("--note-selected"),
    waveDim: get("--wave-dim"),
    /** Main accent (notes, waveform, sample range selection). */
    accent: get("--color-accent"),
    /** Text drawn on top of the accent color. */
    onAccent: get("--color-on-accent"),
    accentSoft: get("--canvas-accent-soft"),
    /** Secondary accent: loop, slice markers, automation curve. */
    blue: get("--color-blue"),
    /** Mouse selection rectangle. */
    selectFill: get("--canvas-select-fill"),
    selectStroke: get("--canvas-select-stroke"),
    /** UI font families for canvas text, so canvases match the interface. */
    font: get("--font-sans") || "sans-serif",
    mono: get("--font-mono") || "monospace",
  };
}

export type Palette = ReturnType<typeof palette>;
