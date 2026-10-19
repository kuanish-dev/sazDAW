// Chord part actions: strum patterns, previewing and building a part. Either the whole part is built or the project is unchanged.
import type { ChordPartReport, ChordPartRequest, StrumPattern } from "../api/types";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface ChordsActions {
  /** The strum patterns (empty if they could not be read). */
  strums(): Promise<StrumPattern[]>;
  /** Builds the part and selects its track; `undefined` on failure (the reason is shown as a notification). */
  build(request: ChordPartRequest): Promise<ChordPartReport | undefined>;
  /** Loops the part without touching the project; `false` on failure (the reason is shown as a notification). */
  preview(request: ChordPartRequest): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createChordsActions(core: Core, views: Views): ChordsActions {
  const { api } = core;
  return {
    async strums() {
      return (await core.run(api.chords.strums())) ?? [];
    },
    preview: (request) => core.attempt(api.chords.preview(request)),
    stopPreview: () => core.exec(api.chords.stopPreview()),
    async build(request) {
      const report = await core.run(api.chords.build(request));
      if (!report) return undefined;
      await views.showCreated({ track: report.track });
      core.toast("info", t().chords.built(report.bars, report.createdTrack));
      return report;
    },
  };
}
