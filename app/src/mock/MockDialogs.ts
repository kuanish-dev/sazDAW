import type { SystemDialogs } from "../api/dialogs";
import { t } from "../i18n";

export class MockDialogs implements SystemDialogs {
  private ask(title: string, defaultPath?: string): string | null {
    const value = window.prompt(title, defaultPath ?? "");
    return value?.trim() ? value.trim() : null;
  }

  async openFile(options: { title: string; defaultPath?: string }) {
    return this.ask(options.title, options.defaultPath);
  }

  async openFiles(options: { title: string; defaultPath?: string }) {
    const value = this.ask(t().mock.commaSeparated(options.title), options.defaultPath);
    return value ? value.split(",").map((p) => p.trim()).filter(Boolean) : [];
  }

  async openDirectory(options: { title: string; defaultPath?: string }) {
    return this.ask(options.title, options.defaultPath);
  }

  async saveFile(options: { title: string; defaultPath?: string }) {
    return this.ask(options.title, options.defaultPath);
  }

  async confirm(message: string) {
    return window.confirm(message);
  }
}
