// Chord part actions: strum patterns, previewing and building a part. Either the whole part is built or the project is unchanged.
import type { ChordPartReport, ChordPartRequest, StrumPattern } from "../api/types";
import { t } from "../i18n";
import type { Backend, Calls, View } from "./context";

export interface ChordsActions {
  /** The strum patterns (empty if they could not be read). */
  strums(): Promise<StrumPattern[]>;
  /** Builds the part and selects its track; `undefined` on failure (the reason is shown as a notification). */
  build(request: ChordPartRequest): Promise<ChordPartReport | undefined>;
  /** Loops the part without touching the project; `false` on failure (the reason is shown as a notification). */
  preview(request: ChordPartRequest): Promise<boolean>;
  stopPreview(): Promise<void>;
}

export function createChordsActions(ctx: Backend & Calls & View): ChordsActions {
  const { api } = ctx;
  return {
    async strums() {
      return (await ctx.run(api.chords.strums())) ?? [];
    },
    preview: (request) => ctx.attempt(api.chords.preview(request)),
    stopPreview: () => ctx.exec(api.chords.stopPreview()),
    async build(request) {
      const report = await ctx.run(api.chords.build(request));
      if (!report) return undefined;
      await ctx.showCreated({ track: report.track });
      ctx.toast("info", t().chords.built(report.bars, report.createdTrack));
      return report;
    },
  };
}
