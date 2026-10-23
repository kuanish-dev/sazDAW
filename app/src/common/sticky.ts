import { createMemo } from "solid-js";

/**
 * A value that remembers the last non-empty one. Needed by `<Show>`/`<Match>` children: in the same update tick
 * when the condition turned false (a clip or track was removed), their effects may still run and read the value;
 * instead of a "stale value" error they get the last known one, and the component is unmounted right after.
 */
export function sticky<T>(source: () => T | null | undefined): () => T | undefined {
  return createMemo<T | undefined>((previous) => source() ?? previous);
}
