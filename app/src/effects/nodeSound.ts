import { createEffect, createMemo, on, onCleanup } from "solid-js";
import type { Id } from "../api/types";
import type { NodeSound } from "./nodeSounds";
import { useEffects } from "./role";

export type { NodeSound };

/** Hears node `node` while the calling component lives: `hear` gets each reading, `onStop` runs when the picture stops
 *  hearing it (closed, or switched to another node) to clear what was shown. */
export function listenToNode(node: () => Id, hear: (sound: NodeSound) => void, onStop?: () => void): void {
  const { nodeSounds } = useEffects();
  const id = createMemo(node);
  createEffect(
    on(id, (current) => {
      const stop = nodeSounds.listen(current, hear);
      onCleanup(() => {
        stop();
        onStop?.();
      });
    }),
  );
}
