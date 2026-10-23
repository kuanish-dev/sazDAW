import { For } from "solid-js";
import type { StrumPattern } from "../api/types";
import { strumArrows } from "./chordPart";
import { t } from "../i18n";

export function StrumPicker(props: { strums: StrumPattern[]; value: string; onChange: (id: string) => void }) {
  const msg = () => t().chords;
  return (
    <div class="flex flex-col gap-1.5">
      <div class="label" title={msg().rhythmHint}>{msg().rhythm}</div>
      <div class="flex flex-wrap gap-2">
        <For each={props.strums}>
          {(strum) => (
            <button
              class="btn !h-auto flex-col !items-start gap-0.5 px-2.5 py-1.5 text-xs" classList={{ "btn-on": props.value === strum.id }}
              aria-pressed={props.value === strum.id} title={msg().rhythmHint} onClick={() => props.onChange(strum.id)}
            >
              <span class="font-semibold">{strum.name}</span>
              <span class="font-mono tracking-[0.3em]">{strumArrows(strum.grid).join("")}</span>
            </button>
          )}
        </For>
      </div>
    </div>
  );
}
