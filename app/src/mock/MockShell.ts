// `AppShell` for the browser without Tauri: zoom is CSS `zoom` on the page root (browsers have no window zoom API).
import type { AppShell } from "../api/shell";
import { t } from "../i18n";

export class MockShell implements AppShell {
  async setZoom(factor: number) {
    document.documentElement.style.zoom = factor === 1 ? "" : String(factor);
  }

  async version() {
    // Only the app knows the real version (`tauri.conf.json`).
    return t().mock.browserVersion;
  }
}
