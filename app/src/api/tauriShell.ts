// `AppShell` implementation for the app: window zoom via the Tauri webview (like ⌘+ in a browser: sharp, no blur),
// version from `tauri.conf.json`.
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
