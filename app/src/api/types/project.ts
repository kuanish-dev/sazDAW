// The project file and everything around it: status, the project feed, recent projects, audio export, error log.
import type { ProjectChanges } from "./model";

export interface Status {
  name: string;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  undoLabel: string | null;
  /** Project file; `null` until the project is saved. */
  path: string | null;
  /** Project folder: holds the project file, samples/, autosave/, exports/. */
  folder: string | null;
}

/** One update of the project feed (the `project` event, `ProjectUpdate` in `daw-rpc/src/feed.rs`). */
export interface ProjectUpdate {
  /** `changes` is the whole project: it becomes the copy. Otherwise they go on top of the update before. */
  whole: boolean;
  changes: ProjectChanges;
  status: Status;
}

/** The undo history as a list (`history.list`): each step is its undo label; the project sits after the last done step. */
export interface HistorySteps {
  /** Steps undo takes back, oldest first: the last one is the next undo. */
  done: string[];
  /** Undone steps redo brings back, the next redo first. */
  undone: string[];
}

export interface RecentProject {
  name: string;
  path: string;
}

export type BitDepth = "int16" | "int24" | "float32";
/** Export file format; the backend sets the file extension from it. FLAC is 16/24-bit only; MP3 ignores bit depth. */
export type ExportFormat = "wav" | "flac" | "mp3";
export type Mp3Bitrate = 128 | 192 | 256 | 320;

/** Match loudness (BS.1770) to `lufs`, with true peak no higher than `truePeakDb` (default −1 dBTP). */
export interface Normalize {
  lufs: number;
  truePeakDb?: number;
}

/** Part of the song: all of it (up to the end of the last clip), the loop region, or a tick range. */
export type ExportRange = "song" | "loop" | { fromTick: number; toTick: number };

/** `project.export` parameters; optional fields default to WAV, no loudness matching, dither for 16-bit, whole song. */
export interface ExportOptions {
  path: string;
  sampleRate: number;
  bitDepth: BitDepth;
  stems: boolean;
  tailSeconds: number;
  format?: ExportFormat;
  mp3Bitrate?: Mp3Bitrate;
  normalize?: Normalize | null;
  dither?: boolean;
  range?: ExportRange;
}

/** Export result: files (the first is the mix), length in frames, mix loudness and true peak (`null` for silence). */
export interface ExportResult {
  files: string[];
  frames: number;
  loudnessLufs: number | null;
  peakDb: number | null;
}

export type LogLevel = "warn" | "error";

/** A new error log record (`log.write`). */
export interface LogRecord {
  level: LogLevel;
  source: string;
  message: string;
  details?: string | null;
}

/** An error log entry (time in Unix milliseconds). */
export interface LogEntry {
  timeMs: number;
  level: LogLevel;
  source: string;
  message: string;
  details: string | null;
}
