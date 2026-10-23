import { For, createMemo, type Accessor } from "solid-js";
import { barPosition, signatureAt } from "../common/tempoMap";
import { usePlayback, useProjectCopy } from "../core/roles";

const MAX_DOTS = 8;

/** The beat under the playhead: its index in the bar (from 0), the bar's beat count, and `flip`, which changes on every
 *  beat — the lit element swaps between two copies of its animation by it, so the animation restarts on each beat. */
export interface Beat {
  beat: number;
  beats: number;
  flip: boolean;
}

/** Follows the playhead on every screen frame but changes only on a new beat; `null` while stopped. */
export function createBeat(): Accessor<Beat | null> {
  const { smoothPlayhead } = usePlayback();
  const { project } = useProjectCopy();
  return createMemo<Beat | null>(() => {
    const at = smoothPlayhead();
    if (at === null) return null;
    const map = project.tempoMap;
    const { bar, beat } = barPosition(map, at);
    const beats = signatureAt(map, at).signature.numerator;
    return { beat: beat - 1, beats, flip: (bar * beats + beat) % 2 === 0 };
  }, null, { equals: (a, b) => a?.beat === b?.beat && a?.beats === b?.beats && a?.flip === b?.flip });
}

export function BeatLights(props: { beat: Beat | null }) {
  const { tick } = usePlayback();
  const beats = () => props.beat?.beats ?? tick().signature.numerator;
  const dots = () => (beats() <= MAX_DOTS ? beats() : 1);
  return (
    <div class="flex items-center gap-[3px] self-center" aria-hidden="true">
      <For each={Array.from({ length: dots() }, (_, i) => i)}>
        {(index) => {
          const beat = () => props.beat;
          const lit = () => beat() !== null && (dots() === 1 || beat()!.beat === index);
          const downbeat = () => (dots() === 1 ? beat()?.beat === 0 : index === 0);
          return (
            <span
              class="h-1.5 w-1.5 rounded-full transition-colors duration-150"
              classList={{
                "bg-panel-3": !lit(),
                "bg-accent shadow-[0_0_6px_var(--color-accent)]": lit() && downbeat(),
                "bg-text shadow-[0_0_5px_var(--color-text)]": lit() && !downbeat(),
                "beat-dot-a": lit() && beat()!.flip,
                "beat-dot-b": lit() && !beat()!.flip,
              }}
            />
          );
        }}
      </For>
    </div>
  );
}
