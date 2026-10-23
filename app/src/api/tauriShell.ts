import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { AppShell } from "./shell";

export class TauriShell implements AppShell {
  setZoom(factor: number) {
    return getCurrentWebview().setZoom(factor);
  }

  version() {
    return getVersion();
  }
}
