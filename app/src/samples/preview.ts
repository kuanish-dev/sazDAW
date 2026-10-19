// Auditioning sound without changing the project: a sample region or a file on disk.
import type { Id } from "../api/types";
import type { Core } from "../core/core";

export interface PreviewActions {
  play(sample: Id, from: number, to: number, looping: boolean): Promise<void>;
  /** Plays a file from disk without adding it to the project. */
  playFile(path: string): Promise<void>;
  stop(): Promise<void>;
  /** Quiet stop (leaving the tab, switching samples): no notification, failures only go to the log. */
  stopQuietly(): void;
}

export function createPreviewActions(core: Core): PreviewActions {
  const { api } = core;
  return {
    play: (sample, from, to, looping) => core.exec(api.preview.play(sample, from, to, looping)),
    playFile: (path) => core.exec(api.preview.playFile(path)),
    stop: () => core.exec(api.preview.stop()),
    stopQuietly: () => void api.preview.stop().catch((error) => console.error("preview.stop", error)),
  };
}
