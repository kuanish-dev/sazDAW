export interface AppShell {
  /** Zoom of the whole interface (1 = unscaled). */
  setZoom(factor: number): Promise<void>;
  /** App version for the About section. */
  version(): Promise<string>;
}
