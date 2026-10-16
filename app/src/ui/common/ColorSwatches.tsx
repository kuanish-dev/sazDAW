// Picking a track or folder color from the project palette: color squares and "no color" (for right-click menus).
import { For } from "solid-js";
import { TRACK_COLORS, type TrackColor } from "../../api/types";
import { t } from "../../i18n";
import { IconClose } from "./icons";
import { TRACK_COLOR_HEX } from "./trackColors";

export function ColorSwatches(props: { current: TrackColor | null | undefined; onPick: (color: TrackColor | null) => void }) {
  return (
    <div class="grid grid-cols-7 gap-1 px-2 py-1">
      <button
        class="flex h-4 w-4 items-center justify-center rounded-sm border border-field-line text-muted" title={t().controls.noColor} aria-label={t().controls.noColor}
        classList={{ "ring-2 ring-accent": !props.current }} onClick={() => props.onPick(null)}
      ><IconClose size={10} /></button>
      <For each={TRACK_COLORS}>
        {(color) => (
          <button
            class="h-4 w-4 rounded-sm" title={t().controls.colors[color]} aria-label={t().controls.colors[color]}
            classList={{ "ring-2 ring-text": props.current === color }} style={{ background: TRACK_COLOR_HEX[color] }}
            onClick={() => props.onPick(color)}
          />
        )}
      </For>
    </div>
  );
}
