import { For, Show } from "solid-js";
import type { PatternSlot } from "../api/types";
import { moveItem } from "./drumPart";
import { MAX_PART_REPEAT, parseRepeat } from "../common/partFields";
import { t } from "../i18n";
import { IconArrowUp, IconClose } from "../common/icons";

export function PartList(props: { slots: PatternSlot[]; nameOf: (pattern: string) => string; onChange: (slots: PatternSlot[]) => void }) {
  const setRepeat = (index: number, text: string) => {
    const repeat = parseRepeat(text);
    props.onChange(props.slots.map((s, i) => (i === index ? { ...s, repeat } : s)));
  };

  return (
    <div class="flex max-h-[40vh] flex-col gap-1 overflow-y-auto">
      <Show when={props.slots.length > 0} fallback={<div class="text-xs text-muted">{t().drums.emptyPart}</div>}>
        <For each={props.slots}>
          {(slot, i) => (
            <div class="flex items-center gap-1 rounded border border-line bg-panel px-2 py-0.5 text-xs">
              <span class="w-5 text-muted">{i() + 1}</span>
              <span class="min-w-0 flex-1 truncate" title={props.nameOf(slot.pattern)}>{props.nameOf(slot.pattern)}</span>
              <span class="text-muted">×</span>
              <input type="number" class="field !h-6 w-12 text-xs" min="1" max={MAX_PART_REPEAT} value={slot.repeat} aria-label={t().drums.repeats}
                onChange={(e) => setRepeat(i(), e.currentTarget.value)} />
              <button class="btn btn-icon !h-6 !w-6 !min-w-6" aria-label={t().drums.up} onClick={() => props.onChange(moveItem(props.slots, i(), -1))}><IconArrowUp size={14} /></button>
              <button class="btn btn-icon !h-6 !w-6 !min-w-6" aria-label={t().drums.down} onClick={() => props.onChange(moveItem(props.slots, i(), 1))}><IconArrowUp size={14} class="rotate-180" /></button>
              <button class="btn btn-icon !h-6 !w-6 !min-w-6 hover:!text-danger" aria-label={t().drums.remove} onClick={() => props.onChange(props.slots.filter((_, j) => j !== i()))}><IconClose size={14} /></button>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
