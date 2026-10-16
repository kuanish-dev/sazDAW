// Label with the sound's name next to the cursor while a sound is dragged (see `dragSample.ts`).
import { Show } from "solid-js";
import { draggedSample } from "./dragSample";
import { IconNote } from "./icons";

export function SampleDragLabel() {
  return (
    <Show when={draggedSample()}>
      {(drag) => (
        <div
          class="pointer-events-none fixed z-50 flex max-w-60 items-center gap-1.5 rounded border border-accent bg-bg px-2 py-1 text-xs shadow-md"
          style={{ left: `${drag().x + 14}px`, top: `${drag().y + 14}px` }}
        ><IconNote size={12} class="text-accent" /><span class="truncate">{drag().label}</span></div>
      )}
    </Show>
  );
}
