// Window shell contract: things done by the app window itself rather than the backend (page zoom, app version).
// Implementations: `tauriShell.ts` (app) and `mock/MockShell.ts` (browser); chosen in `composition.ts`.

export interface AppShell {
  /** Zoom of the whole interface (1 = unscaled). */
  setZoom(factor: number): Promise<void>;
  /** App version for the About section. */
  version(): Promise<string>;
}
