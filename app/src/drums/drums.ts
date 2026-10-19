// Drum part actions: ready-made patterns, previewing and building a part. Either the whole part is built or the project is unchanged.
import type { DrumPartReport, DrumPartRequest, DrumPattern } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface Drums {
  /** All library patterns (empty if they could not be read). */
  patterns(): Promise<DrumPattern[]>;
  /** Builds the part and selects its first track; `undefined` on failure (the reason is shown as a notification). */
  build(request: DrumPartRequest): Promise<DrumPartReport | undefined>;
  /** Loops the part without touching the project; `false` on failure (the reason is shown as a notification). */
  preview(request: DrumPartRequest): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createDrums(core: Core, views: Views): Drums {
  const { api } = core;
  return {
    async patterns() {
      return (await core.calls.run(api.patterns.list())) ?? [];
    },
    preview: (request) => core.calls.attempt(api.drums.preview(request)),
    stopPreview: () => core.calls.exec(api.drums.stopPreview()),
    async build(request) {
      const report = await core.calls.run(api.drums.build(request));
      if (!report) return undefined;
      // No new track to show: the project change event re-reads the project by itself.
      if (report.tracks[0] !== undefined) await views.showCreated({ track: report.tracks[0] });
      core.notifications.toast("info", t().drums.built(report.bars, report.createdTracks));
      return report;
    },
  };
}
