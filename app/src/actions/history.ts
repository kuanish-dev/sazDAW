// Edit history: undo and redo of project edits (each backend edit or group is one step). Errors are notifications.
import type { Backend, Calls } from "./context";

export interface HistoryActions {
  undo(): Promise<void>;
  redo(): Promise<void>;
}

export function createHistoryActions(ctx: Backend & Calls): HistoryActions {
  const { api } = ctx;
  return {
    undo: () => ctx.exec(api.history.undo()),
    redo: () => ctx.exec(api.history.redo()),
  };
}
