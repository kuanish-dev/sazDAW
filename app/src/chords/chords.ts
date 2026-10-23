import type { ChordPartReport, ChordPartRequest, StrumPattern } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface Chords {
  /** The strum patterns (empty if they could not be read). */
  strums(): Promise<StrumPattern[]>;
  /** Builds the part and selects its track; `undefined` on failure (the reason is shown as a notification). */
  build(request: ChordPartRequest): Promise<ChordPartReport | undefined>;
  /** Loops the part without touching the project; `false` on failure (the reason is shown as a notification). */
  preview(request: ChordPartRequest): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createChords(core: Core, views: Views): Chords {
  const { api } = core;
  return {
    async strums() {
      return (await core.calls.run(api.chords.strums())) ?? [];
    },
    preview: (request) => core.calls.attempt(api.chords.preview(request)),
    stopPreview: () => core.calls.exec(api.chords.stopPreview()),
    async build(request) {
      const report = await core.calls.run(api.chords.build(request));
      if (!report) return undefined;
      await views.showCreated({ track: report.track });
      core.notifications.toast("info", t().chords.built(report.bars, report.createdTrack));
      return report;
    },
  };
}
