import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import type { ConfirmButtons, SystemDialogs } from "./dialogs";

export class TauriDialogs implements SystemDialogs {
  async openFile(options: Parameters<SystemDialogs["openFile"]>[0]) {
    const result = await open({ ...options, multiple: false, directory: false });
    return typeof result === "string" ? result : null;
  }

  async openFiles(options: Parameters<SystemDialogs["openFiles"]>[0]) {
    const result = await open({ ...options, multiple: true, directory: false });
    return Array.isArray(result) ? result : typeof result === "string" ? [result] : [];
  }

  async openDirectory(options: Parameters<SystemDialogs["openDirectory"]>[0]) {
    const result = await open({ ...options, multiple: false, directory: true });
    return typeof result === "string" ? result : null;
  }

  saveFile(options: Parameters<SystemDialogs["saveFile"]>[0]) {
    return save(options);
  }

  confirm(message: string, title: string, buttons?: ConfirmButtons) {
    return confirm(message, { title, kind: "warning", okLabel: buttons?.ok, cancelLabel: buttons?.cancel });
  }
}
