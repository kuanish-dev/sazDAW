// Beat lights next to the position counter: one dot per beat of the bar, the first in the accent color. While playing the
// current beat's dot flashes; paused, it stays lit; stopped, all are dim. A bar of more than `MAX_DOTS` beats shows a
// single dot that flashes on every beat. `createBeat` is the beat clock the lights and the Play button pulse with.
import { For, createMemo, type Accessor } from "solid-js";
import { barPosition, signatureAt } from "../common/tempoMap";
import { useSession } from "../session";

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
  const session = useSession();
  return createMemo<Beat | null>(() => {
    const at = session.smoothPlayhead();
    if (at === null) return null;
    const map = session.project.tempoMap;
    const { bar, beat } = barPosition(map, at);
    const beats = signatureAt(map, at).signature.numerator;
    // Beats counted from the song start: consecutive beats always differ in parity, across bar lines too.
    return { beat: beat - 1, beats, flip: (bar * beats + beat) % 2 === 0 };
  }, null, { equals: (a, b) => a?.beat === b?.beat && a?.beats === b?.beats && a?.flip === b?.flip });
}

export function BeatLights(props: { beat: Beat | null }) {
  const session = useSession();
  const beats = () => props.beat?.beats ?? session.tick().signature.numerator;
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
