import { createSignal } from "solid-js";
import type { Notifications } from "./notifications";

export interface Calls {
  /** Runs a call; a failure is shown (`showFailure`) and the whole project is asked for again. */
  run<T>(action: Promise<T>): Promise<T | undefined>;
  /** Like `run`, but returns whether the action succeeded. */
  attempt(action: Promise<unknown>): Promise<boolean>;
  /** Like `run`, for commands without a result: await and forget. */
  exec(action: Promise<unknown>): Promise<void>;
  /** Shows the loading indicator while the action runs. */
  withBusy<T>(text: string, action: () => Promise<T>): Promise<T>;
  /** Label of the long operation running now (opening a project); `null` — none. */
  busy(): string | null;
}

export function createCalls(notifications: Pick<Notifications, "showFailure">, reload: () => Promise<void>): Calls {
  const [busy, setBusy] = createSignal<string | null>(null);

  /** The call's result, or `failed` once the failure is shown and the whole project asked for. */
  const settle = async <T>(action: Promise<T>): Promise<{ value: T } | "failed"> => {
    try {
      return { value: await action };
    } catch (error) {
      notifications.showFailure(error);
      await reload();
      return "failed";
    }
  };

  return {
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
    busy,
  };
}
