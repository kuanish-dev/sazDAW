import type { LogEntry } from "../../api/types";
import type { HandlerSet } from "./types";

export function logHandlers(): HandlerSet {
  const entries: LogEntry[] = [];
  const h: HandlerSet = {};
  h["log.write"] = ({ level, source, message, details }) => {
    entries.push({ timeMs: Date.now(), level, source, message, details: details ?? null });
  };
  h["log.recent"] = ({ limit }) => entries.slice(-Math.max(0, limit));
  h["log.file"] = () => "~/Library/Application Support/DAW/logs/errors.log";
  return h;
}
