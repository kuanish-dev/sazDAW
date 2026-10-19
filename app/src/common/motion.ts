// The system's "reduce motion" setting for animation driven from code (canvas loops). CSS animations follow the same
// setting in `index.css`.

export const prefersReducedMotion = (): boolean =>
  typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
