// The live sound of the node a picture shows (an effect editor's graph, or an effect along the signal path): a picture
// hears its node through the app's registry (`effects/nodeSounds.ts`) while it lives, and again for another node when it
// switches. The registry captures every node shown at once and reads them in one batched call; when playback stops the
// nodes go silent and every picture fades out by itself from the silence it hears.
import { createEffect, createMemo, on, onCleanup } from "solid-js";
import type { Id } from "../api/types";
import type { NodeSound } from "./nodeSounds";
import { useSession } from "../session";

export type { NodeSound };

/** Hears node `node` while the calling component lives: `hear` gets each reading, `onStop` runs when the picture stops
 *  hearing it (closed, or switched to another node) to clear what was shown. */
export function listenToNode(node: () => Id, hear: (sound: NodeSound) => void, onStop?: () => void): void {
  const { nodeSounds } = useSession();
  // The memo passes on only a real change of the node (the panel's node object is rebuilt on every project update).
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
