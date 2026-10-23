import { For, Index } from "solid-js";
import { type ChordCell, addBar, removeBar, toggleSplit } from "./chordPart";
import { t } from "../i18n";

export function ProgressionBars(props: {
  bars: string[][];
  selected: ChordCell;
  onSelect: (cell: ChordCell) => void;
  onChange: (bars: string[][]) => void;
}) {
  const msg = () => t().chords;
  const isSelected = (bar: number, half: number) => props.selected.bar === bar && props.selected.half === half;
  return (
    <div class="flex flex-col gap-1.5">
      <div class="label" title={msg().progressionHint}>{msg().progression}</div>
      <div class="flex flex-wrap gap-1.5">
        <For each={props.bars}>
          {(chords, bar) => (
            <div class="flex flex-col gap-0.5 rounded border border-line p-1" role="group" aria-label={msg().bar(bar() + 1)}>
              <div class="flex gap-0.5">
                <Index each={chords}>
                  {(symbol, half) => (
                    <button
                      class="btn !h-9 min-w-12 text-sm font-semibold" classList={{ "btn-on": isSelected(bar(), half) }}
                      aria-pressed={isSelected(bar(), half)} onClick={() => props.onSelect({ bar: bar(), half })}
                    >
                      {symbol()}
                    </button>
                  )}
                </Index>
              </div>
              <div class="flex items-center justify-between gap-1 text-[10px] text-muted">
                <span class="px-0.5">{bar() + 1}</span>
                <span class="flex">
                  <button
                    class="btn btn-ghost !h-4 !min-w-0 !px-1 text-[10px]" title={chords.length === 2 ? msg().join : msg().split}
                    onClick={() => props.onChange(toggleSplit(props.bars, bar()))}
                  >
                    ½
                  </button>
                  <button
                    class="btn btn-ghost !h-4 !min-w-0 !px-1 text-[10px]" title={msg().removeBar} disabled={props.bars.length === 1}
                    onClick={() => props.onChange(removeBar(props.bars, bar()))}
                  >
                    ×
                  </button>
                </span>
              </div>
            </div>
          )}
        </For>
        <button class="btn !h-auto text-xs" onClick={() => props.onChange(addBar(props.bars))}>{msg().addBar}</button>
      </div>
      <div class="text-[11px] text-muted">{msg().progressionHint}</div>
    </div>
  );
}
