// Audio export without side effects: the choices of the export form and which apply to which format (as the backend
// accepts them), the request built from the form, the file extension for a format, the range of the selected clips and a
// short result summary for the message.
import type { BitDepth, Clip, ExportFormat, ExportOptions, ExportRange, ExportResult, Id, Mp3Bitrate, Normalize } from "../api/types";
import { t } from "../i18n";

export const EXPORT_FORMATS: readonly ExportFormat[] = ["wav", "flac", "mp3"] as const;
export const SAMPLE_RATES: readonly number[] = [44_100, 48_000, 96_000] as const;
export const DEFAULT_SAMPLE_RATE = 44_100;
export const BIT_DEPTHS: readonly BitDepth[] = ["int16", "int24", "float32"] as const;
export const MP3_BITRATES: readonly Mp3Bitrate[] = [128, 192, 256, 320] as const;
export const DEFAULT_CEILING_DB = -1;
/** Tail after the end, s: lets reverb and delay die out. */
export const DEFAULT_TAIL_SECONDS = 2;
export const MAX_TAIL_SECONDS = 30;

/** Loudness targets: `off` leaves the level alone, `custom` takes the number from the field. */
export type LoudnessChoice = "off" | "-14" | "-16" | "-9" | "custom";
export const LOUDNESS_CHOICES: readonly { value: LoudnessChoice; lufs?: number }[] = [
  { value: "off" },
  { value: "-14", lufs: -14 },
  { value: "-16", lufs: -16 },
  { value: "-9", lufs: -9 },
  { value: "custom" },
] as const;

export type RangeChoice = "song" | "loop" | "selection";

/** Bit depths `format` can hold: FLAC has no float (MP3 has no bit depth of its own). */
export const bitDepthsFor = (format: ExportFormat): BitDepth[] => BIT_DEPTHS.filter((d) => !(d === "float32" && format === "flac"));

/** The bit depth kept after switching to `format`: one it can't hold becomes 24-bit. */
export const fitBitDepth = (format: ExportFormat, depth: BitDepth): BitDepth => (bitDepthsFor(format).includes(depth) ? depth : "int24");

/** Dither applies to integer PCM only: not MP3, not float. */
export const isIntegerPcm = (format: ExportFormat, depth: BitDepth): boolean => format !== "mp3" && depth !== "float32";

/** The backend's dither when the user hasn't chosen: on for 16-bit, off for 24-bit. */
export const defaultDither = (depth: BitDepth): boolean => depth === "int16";

/** The export form as the user filled it. `dither: null` — the default for the bit depth. */
export interface ExportForm {
  path: string;
  format: ExportFormat;
  sampleRate: number;
  bitDepth: BitDepth;
  bitrate: Mp3Bitrate;
  dither: boolean | null;
  loudness: LoudnessChoice;
  customLufs: number;
  ceilingDb: number;
  range: RangeChoice;
  stems: boolean;
  tailSeconds: number;
}

/** The loudness target in LUFS; `undefined` — no matching. */
export function targetLufs(form: Pick<ExportForm, "loudness" | "customLufs">): number | undefined {
  return form.loudness === "custom" ? form.customLufs : LOUDNESS_CHOICES.find((l) => l.value === form.loudness)?.lufs;
}

/**
 * The request for the form: settings the format doesn't use are left out (bitrate except MP3, dither except integer PCM);
 * "selected clips" without a selection (`selection: null`) is the whole song.
 */
export function exportOptions(form: ExportForm, selection: { fromTick: number; toTick: number } | null): ExportOptions {
  const lufs = targetLufs(form);
  const range: ExportRange = form.range === "selection" ? (selection ?? "song") : form.range;
  return {
    path: form.path,
    sampleRate: form.sampleRate,
    bitDepth: form.bitDepth,
    stems: form.stems,
    tailSeconds: form.tailSeconds,
    format: form.format,
    mp3Bitrate: form.format === "mp3" ? form.bitrate : undefined,
    normalize: lufs === undefined ? null : { lufs, truePeakDb: form.ceilingDb },
    dither: isIntegerPcm(form.format, form.bitDepth) ? (form.dither ?? defaultDither(form.bitDepth)) : undefined,
    range,
  };
}

/** Path with the format's extension: an audio extension is replaced, any other is kept (`song.v2` → `song.v2.flac`). */
export function pathForFormat(path: string, format: ExportFormat): string {
  const match = /\.([^./\\]+)$/.exec(path);
  const known = match && EXPORT_FORMATS.includes(match[1]!.toLowerCase() as ExportFormat);
  return known ? `${path.slice(0, match.index)}.${format}` : `${path}.${format}`;
}

/** From the start of the first to the end of the last of clips `ids`; `null` if there are no such clips. */
export function clipsRange(clips: Clip[], ids: Id[]): { fromTick: number; toTick: number } | null {
  const chosen = clips.filter((c) => ids.includes(c.id));
  if (chosen.length === 0) return null;
  return {
    fromTick: Math.min(...chosen.map((c) => c.startTick)),
    toTick: Math.max(...chosen.map((c) => c.startTick + c.lengthTicks)),
  };
}

/** How far below the loudness target is still not worth mentioning the ceiling: rounding and BS.1770 gating. */
const TARGET_TOLERANCE_LU = 0.5;

/** "−14.0 LUFS, peak −1.0 dBTP", plus a note if the peak ceiling kept it from the target; `null` when there is no loudness (silence). */
export function loudnessSummary(result: ExportResult, target?: Normalize | null): string | null {
  if (result.loudnessLufs === null) return null;
  const peak = result.peakDb === null ? null : result.peakDb.toFixed(1);
  const short = target && result.loudnessLufs < target.lufs - TARGET_TOLERANCE_LU;
  return t().project.loudness(result.loudnessLufs.toFixed(1), peak, short ? target.lufs : null);
}
