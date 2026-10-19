// Edit history: undo and redo of project edits (each backend edit or group is one step), the list of steps for the
// history panel and walking to a step in it. Errors are notifications.
import type { HistorySteps } from "../api/types";
import type { Core } from "../core/core";

export interface EditHistory {
  undo(): Promise<void>;
  redo(): Promise<void>;
  /** The undo history to show: done steps oldest first, undone ones in redo order; `undefined` after a failure (shown,
   *  without the usual re-read: the panel reads the list after every re-read, so a re-read would loop). */
  steps(): Promise<HistorySteps | undefined>;
  /** Walks `count` steps: back by undo when negative, forward by redo when positive. It stops early where the backend
   *  refuses a step: the history ended, or a group (a recording) is open. */
  travel(count: number): Promise<void>;
}

export function createEditHistory(core: Core): EditHistory {
  const { api } = core;
  // One step at a time, as the undo/redo buttons do: every step stays an ordinary undo or redo for the backend.
  const walk = async (count: number) => {
    const step = count < 0 ? api.history.undo : api.history.redo;
    for (let i = 0; i < Math.abs(count); i++) {
      if (!(await step())) return;
    }
  };
  return {
    undo: () => core.calls.exec(api.history.undo()),
    redo: () => core.calls.exec(api.history.redo()),
    steps: () => api.history.list().catch((error: unknown) => {
      core.notifications.showFailure(error);
      return undefined;
    }),
    travel: (count) => core.calls.exec(walk(count)),
  };
}
