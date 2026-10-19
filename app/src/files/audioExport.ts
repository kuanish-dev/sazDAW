// Audio export: rendering the song (or a part of it) to a file, and where that file goes by default.
import { FLAC_FILTER, MP3_FILTER, WAV_FILTER } from "../api/dialogs";
import type { ExportFormat, ExportOptions, ExportResult } from "../api/types";
import { loudnessSummary, pathForFormat } from "./exportAudio";
import { joinPath } from "../common/format";
import { t } from "../i18n";
import type { Core } from "../core/core";
import { homeDir } from "./paths";

const EXPORT_FILTERS = { wav: WAV_FILTER, flac: FLAC_FILTER, mp3: MP3_FILTER } as const;

export interface AudioExport {
  /** Renders to an audio file; on success, a "Done: …" toast with the mix loudness and peak. */
  render(options: ExportOptions): Promise<ExportResult | undefined>;
  /** Default path: `<project folder>/exports/<name>.<format>`, otherwise in the home folder. */
  suggestedPath(format: ExportFormat): Promise<string>;
  /** "Export to" dialog; null if cancelled. The format's extension is added automatically. */
  choosePath(format: ExportFormat, current?: string): Promise<string | null>;
}

export function createAudioExport(core: Core): AudioExport {
  const { api, dialogs } = core;
  return {
    async render(options) {
      const result = await core.calls.run(api.project.exportAudio(options));
      if (result) {
        const seconds = (result.frames / options.sampleRate).toFixed(1);
        core.notifications.toast("info", t().project.exported(result.files.length, seconds, loudnessSummary(result, options.normalize)));
      }
      return result;
    },

    async suggestedPath(format) {
      const { folder, name } = core.copy.status();
      const fileName = `${name || "export"}.${format}`;
      return joinPath(folder ? joinPath(folder, "exports") : await homeDir(api), fileName);
    },

    async choosePath(format, current) {
      const filters = [EXPORT_FILTERS[format]];
      const chosen = await dialogs.saveFile({ title: t().project.exportPathTitle, filters, defaultPath: current || undefined });
      return chosen ? pathForFormat(chosen, format) : null;
    },
  };
}
