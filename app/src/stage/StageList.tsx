// Every channel that can stand on the stage (tracks in mixer order, then buses): on the stage — select it, take it
// off; off the stage — place it, or drag it into the room.
import { For, Show } from "solid-js";
import type { Id } from "../api/types";
import type { StageChannel } from "./stageRules";
import { t } from "../i18n";
import { STAGE_CHANNEL_MIME } from "./stagePointer";

export function StageList(props: {
  channels: StageChannel[];
  colorOf: (node: Id) => string;
  selected: Id | null;
  onSelect: (node: Id) => void;
  onPlace: (channel: StageChannel) => void;
  onTakeOff: (node: Id) => void;
}) {
  return (
    <div class="min-h-0 flex-1 overflow-y-auto">
      <div class="section-header px-3 py-1">{t().stage.channels}</div>
      <For each={props.channels} fallback={<p class="px-3 py-2 text-xs text-muted">{t().stage.empty}</p>}>
        {(channel) => (
          <div
            class="flex h-7 items-center gap-2 px-3 text-xs"
            classList={{ "bg-select": props.selected === channel.node, "cursor-grab hover:bg-hover": !channel.spot, "cursor-pointer hover:bg-hover": !!channel.spot }}
            draggable={!channel.spot}
            onDragStart={(e) => e.dataTransfer?.setData(STAGE_CHANNEL_MIME, String(channel.node))}
            onClick={() => channel.spot && props.onSelect(channel.node)}
          >
            <span class="size-2.5 shrink-0 rounded-full" classList={{ "opacity-40": !channel.spot }} style={{ background: props.colorOf(channel.node) }} />
            <span class="min-w-0 flex-1 truncate" classList={{ "text-muted": !channel.spot }}>{channel.name}</span>
            <Show when={channel.track === null}><span class="text-[10px] uppercase text-muted">{t().stage.bus}</span></Show>
            <Show
              when={channel.spot}
              fallback={
                <button class="btn btn-ghost !h-5 !px-1.5 text-[11px]" title={t().stage.placeHint(channel.name)} onClick={() => props.onPlace(channel)}>
                  {t().stage.place}
                </button>
              }
            >
              <button
                class="btn btn-ghost !h-5 !px-1.5 text-[11px] text-muted"
                title={t().stage.takeOffHint(channel.name)}
                onClick={(e) => {
                  e.stopPropagation();
                  props.onTakeOff(channel.node);
                }}
              >
                {t().stage.takeOff}
              </button>
            </Show>
          </div>
        )}
      </For>
    </div>
  );
}
