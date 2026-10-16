// Drum part actions: ready-made patterns, previewing and building a part. Either the whole part is built or the project is unchanged.
import type { DrumPartReport, DrumPartRequest, DrumPattern } from "../api/types";
import { t } from "../i18n";
import type { Backend, Calls, View } from "./context";

export interface DrumsActions {
  /** All library patterns (empty if they could not be read). */
  patterns(): Promise<DrumPattern[]>;
  /** Builds the part and selects its first track; `undefined` on failure (the reason is shown as a notification). */
  build(request: DrumPartRequest): Promise<DrumPartReport | undefined>;
  /** Loops the part without touching the project; `false` on failure (the reason is shown as a notification). */
  preview(request: DrumPartRequest): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createDrumsActions(ctx: Backend & Calls & View): DrumsActions {
  const { api } = ctx;
  return {
    async patterns() {
      return (await ctx.run(api.patterns.list())) ?? [];
    },
    preview: (request) => ctx.attempt(api.drums.preview(request)),
    stopPreview: () => ctx.exec(api.drums.stopPreview()),
    async build(request) {
      const report = await ctx.run(api.drums.build(request));
      if (!report) return undefined;
      // No new track to show: the project change event re-reads the project by itself.
      if (report.tracks[0] !== undefined) await ctx.showCreated({ track: report.tracks[0] });
      ctx.toast("info", t().drums.built(report.bars, report.createdTracks));
      return report;
    },
  };
}
