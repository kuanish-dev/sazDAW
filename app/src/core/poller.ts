import { onCleanup } from "solid-js";

export interface Poller {
  /** Starts polling (an already running poll is not duplicated); the first request is immediate. */
  start(): void;
  stop(): void;
  running(): boolean;
}

/** Calls `poll` every `intervalMs`; the next call does not start until the previous one finishes. */
export function createPoller(poll: () => Promise<void>, intervalMs: number): Poller {
  let timer: ReturnType<typeof setInterval> | null = null;
  let busy = false;

  const tick = async () => {
    if (busy) return;
    busy = true;
    try {
      await poll();
    } finally {
      busy = false;
    }
  };

  return {
    start() {
      void tick();
      if (timer === null) timer = setInterval(() => void tick(), intervalMs);
    },
    stop() {
      if (timer !== null) clearInterval(timer);
      timer = null;
    },
    running: () => timer !== null,
  };
}

/** How to open a backend capture, read it and show what was read. */
export interface WatchedCapture<T> {
  /** Opens (`true`) or closes the capture (the input level). None for a capture the backend keeps going while it is read
   *  (`CAPTURE_LEASE_SECONDS`: the master window, the master meter): stopping the polling is enough. */
  watch?(on: boolean): Promise<void>;
  /** One reading; `undefined` — it failed (the action has shown why). */
  read(): Promise<T | undefined>;
  show(value: T): void;
  /** After the capture closes (to clear what was shown). */
  onStop?(): void;
}

/**
 * A backend capture that is open only while it is polled (the input level check, the master visualizer, the master
 * meter): `start` opens it and polls, `stop` closes it. A failed reading stops it, so errors don't pile up; a reading
 * that lands after `stop` is dropped. Leaving the owner (component cleanup) stops it — a capture must not stay open
 * behind a closed view.
 */
export function createWatchedPoll<T>(capture: WatchedCapture<T>, intervalMs: number): Poller {
  const poller = createPoller(async () => {
    const next = await capture.read();
    if (!poller.running()) return;
    if (next !== undefined) capture.show(next);
    else stop();
  }, intervalMs);

  const stop = () => {
    if (!poller.running()) return;
    poller.stop();
    capture.onStop?.();
    void capture.watch?.(false);
  };
  onCleanup(stop);

  return {
    start() {
      if (poller.running()) return;
      poller.start();
      void capture.watch?.(true);
    },
    stop,
    running: poller.running,
  };
}
