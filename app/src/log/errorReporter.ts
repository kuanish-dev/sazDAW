// Interface error collection: everything uncaught and everything sent to console.error goes to the log on disk.
// Job: turn any error into a record, don't repeat the same one in a row, and never break the app itself.
import type { LogLevel, LogRecord } from "../api/types";

export interface ErrorReporter {
  report(level: LogLevel, source: string, error: unknown, details?: string): void;
  /** Subscribes to global window errors and console.error; returns the unsubscribe function. */
  install(target: Pick<Window, "addEventListener" | "removeEventListener">, output?: Console): () => void;
}

/** The same error is not recorded again within this window (the interface may emit it every frame). */
const REPEAT_WINDOW_MS = 2_000;
/** How many distinct errors to remember for repeat suppression; beyond that the memory is simply reset. */
const MAX_REMEMBERED = 200;

function describe(error: unknown): { message: string; stack: string | null } {
  if (error instanceof Error) return { message: `${error.name}: ${error.message}`, stack: error.stack ?? null };
  if (typeof error === "string") return { message: error, stack: null };
  try {
    return { message: JSON.stringify(error) ?? String(error), stack: null };
  } catch {
    return { message: String(error), stack: null };
  }
}

export function createErrorReporter(write: (record: LogRecord) => Promise<unknown>, now: () => number = Date.now): ErrorReporter {
  const lastSeen = new Map<string, number>();
  /** The console before patching: a failure of the log itself is printed there, otherwise it would loop back into the log. */
  let fallback: Pick<Console, "error"> = console;
  /** A synchronous `report` call is in progress: writing the log may itself call console.error — avoid a loop. */
  let reporting = false;

  const report: ErrorReporter["report"] = (level, source, error, details) => {
    if (reporting) return;
    const { message, stack } = describe(error);
    const key = `${source}|${message}`;
    const time = now();
    const previous = lastSeen.get(key);
    if (previous !== undefined && time - previous < REPEAT_WINDOW_MS) return;
    lastSeen.set(key, time);
    if (lastSeen.size > MAX_REMEMBERED) lastSeen.clear();
    const joined = [details, stack].filter(Boolean).join("\n");
    reporting = true;
    try {
      void write({ level, source, message, details: joined || null })
        .catch((failure) => fallback.error("Error log unavailable:", failure, "— not recorded:", message));
    } finally {
      reporting = false;
    }
  };

  const install: ErrorReporter["install"] = (target, output = console) => {
    const onError = (event: Event) => {
      const e = event as ErrorEvent;
      report("error", "frontend", e.error ?? e.message ?? "window error", e.filename ? `${e.filename}:${e.lineno}:${e.colno}` : undefined);
    };
    const onRejection = (event: Event) => {
      report("error", "frontend", (event as PromiseRejectionEvent).reason ?? "promise rejected without a reason", "unhandled rejection");
    };
    target.addEventListener("error", onError);
    target.addEventListener("unhandledrejection", onRejection);
    // Errors the framework only prints to the console are recorded too.
    const original = output.error.bind(output);
    fallback = { error: original };
    output.error = (...args: unknown[]) => {
      original(...args);
      const stack = args.find((a): a is Error => a instanceof Error)?.stack;
      report("error", "console", args.map((a) => describe(a).message).join(" "), stack);
    };
    return () => {
      target.removeEventListener("error", onError);
      target.removeEventListener("unhandledrejection", onRejection);
      output.error = original;
      fallback = output;
    };
  };

  return { report, install };
}
