// How a call to the backend runs and how the user hears about it: a failure becomes a notification and the project copy
// is re-read whole (the refused value may already be on screen, `patchProject`); a long operation shows the loading
// indicator (`busy`). Part of the shared core (`core.ts`).
import { type Accessor, createSignal } from "solid-js";
import type { Toasts } from "./notifications";

export interface Calls {
  /** Runs a call; a failure is shown (`showFailure`) and the project is re-read. */
  run<T>(action: Promise<T>): Promise<T | undefined>;
  /** Like `run`, but returns whether the action succeeded. */
  attempt(action: Promise<unknown>): Promise<boolean>;
  /** Like `run`, for commands without a result: await and forget. */
  exec(action: Promise<unknown>): Promise<void>;
  /** Shows the loading indicator while the action runs. */
  withBusy<T>(text: string, action: () => Promise<T>): Promise<T>;
  /** Notification; `error` is also written to the error log. */
  toast(kind: "error" | "info", text: string): void;
  /** Shows a failed call: a backend refusal only as a toast (the backend logged it), anything else also goes to the log. */
  showFailure(error: unknown): void;
}

export function createCalls(toasts: Pick<Toasts, "toast" | "showFailure">, reload: () => Promise<void>): { calls: Calls; busy: Accessor<string | null> } {
  /** Label of a long operation (opening a project); while set, the loading indicator is shown. */
  const [busy, setBusy] = createSignal<string | null>(null);

  /** The call's result, or `failed` once the failure is shown and the copy re-read. */
  const settle = async <T>(action: Promise<T>): Promise<{ value: T } | "failed"> => {
    try {
      return { value: await action };
    } catch (error) {
      toasts.showFailure(error);
      await reload();
      return "failed";
    }
  };

  const calls: Calls = {
    async run<T>(action: Promise<T>) {
      const result = await settle(action);
      return result === "failed" ? undefined : result.value;
    },
    attempt: async (action) => (await settle(action)) !== "failed",
    exec: async (action) => void (await settle(action)),
    async withBusy<T>(text: string, action: () => Promise<T>) {
      setBusy(text);
      try {
        return await action();
      } finally {
        setBusy(null);
      }
    },
    toast: toasts.toast,
    showFailure: toasts.showFailure,
  };
  return { calls, busy };
}
