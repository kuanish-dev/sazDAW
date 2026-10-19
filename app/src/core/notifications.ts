// Notifications (toasts) in the window corner: show, remove on timeout or by button; the message about a finished note take.
import { createEffect, createSignal } from "solid-js";
import type { RecordState } from "../api/types";
import { t } from "../i18n";
import { errorText, isBackendError } from "../i18n/errors";

export interface Toast {
  id: number;
  kind: "error" | "info";
  text: string;
}

/** Notification lifetime, ms: an error must stay long enough to be read. */
const ERROR_TOAST_MS = 6000;
const INFO_TOAST_MS = 2500;

/** What the UI reads of the notifications. */
export interface ToastReads {
  toasts(): Toast[];
  dismissToast(id: number): void;
}

/** The notifications: what the UI reads, and how the roles show something. */
export type Notifications = ToastReads & {
  /** A notification from the interface: an error also goes to the log (console.error → errorReporter), otherwise it would be lost. */
  toast(kind: Toast["kind"], text: string): void;
  /**
   * A failed call. A backend refusal is only shown: `daw-rpc` has already logged it. Anything else (the IPC link, a throw
   * in our own code, a failed re-read) was logged by no one, so it goes to the log too.
   */
  showFailure(error: unknown): void;
};

export function createNotifications(): Notifications {
  const [toasts, setToasts] = createSignal<Toast[]>([]);
  let lastId = 0;

  const dismissToast = (id: number) => setToasts((list) => list.filter((t) => t.id !== id));

  /** Backend errors are logged by the backend itself — use `showFailure` for them. */
  const show = (kind: Toast["kind"], text: string) => {
    const id = ++lastId;
    setToasts((list) => [...list, { id, kind, text }]);
    setTimeout(() => dismissToast(id), kind === "error" ? ERROR_TOAST_MS : INFO_TOAST_MS);
  };

  return {
    toasts,
    dismissToast,
    toast(kind, text) {
      if (kind === "error") console.error(text);
      show(kind, text);
    },
    showFailure(error) {
      if (!isBackendError(error)) console.error(error);
      show("error", errorText(error));
    },
  };
}

/** "Recorded N notes" after a note recording take (watches the record state in the `tick` stream). */
export function createTakeNotice(record: () => RecordState, toast: (kind: "info", text: string) => void) {
  let wasCapturing = false;
  createEffect(() => {
    const { capturing, takeNotes } = record();
    if (wasCapturing && !capturing && takeNotes > 0) toast("info", t().shell.notesRecorded(takeNotes));
    wasCapturing = capturing;
  });
}
