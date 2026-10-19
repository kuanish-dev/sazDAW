// Reads the error log for the "Error log" window. A failure of the log itself must not produce new notifications.
import type { LogEntry } from "../api/types";
import type { Core } from "../core/core";

export interface LogActions {
  /** Latest entries, newest last; an empty list if reading fails. */
  recent(limit: number): Promise<LogEntry[]>;
  /** Path to the log file (so it can be found on disk). */
  file(): Promise<string | null>;
}

export function createLogActions(core: Core): LogActions {
  const { api } = core;
  return {
    // The log is read from disk: a read failure is not written to the log (that would loop), only to the console.
    recent: (limit) => api.log.recent(limit).catch((error) => { console.warn("log.recent", error); return []; }),
    file: () => api.log.file().catch((error) => { console.warn("log.file", error); return null; }),
  };
}
