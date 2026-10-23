import type { LogEntry } from "../api/types";
import type { Core } from "../core/core";

export interface ErrorLog {
  /** Latest entries, newest last; an empty list if reading fails. */
  recent(limit: number): Promise<LogEntry[]>;
  /** Path to the log file (so it can be found on disk). */
  file(): Promise<string | null>;
}

export function createErrorLog(core: Core): ErrorLog {
  const { api } = core;
  return {
    recent: (limit) => api.log.recent(limit).catch((error) => { console.warn("log.recent", error); return []; }),
    file: () => api.log.file().catch((error) => { console.warn("log.file", error); return null; }),
  };
}
