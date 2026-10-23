import type { AppShell } from "../api/shell";
import { t } from "../i18n";

export class MockShell implements AppShell {
  async setZoom(factor: number) {
    document.documentElement.style.zoom = factor === 1 ? "" : String(factor);
  }

  async version() {
    return t().mock.browserVersion;
  }
}
