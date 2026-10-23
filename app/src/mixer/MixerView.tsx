import { For, Show, createMemo, createSignal } from "solid-js";
import type { Id } from "../api/types";
import { stripOrder } from "./mixerStrips";
import { t } from "../i18n";
import { createStreamRate } from "../common/streamRate";
import { ChannelStrip, MasterStrip } from "./ChannelStrip";
import { EffectPopover, type OpenEffect } from "./EffectPopover";
import { MasterMeters } from "./MasterMeters";
import { useProjectCopy } from "../core/roles";

export function MixerView() {
  const { project } = useProjectCopy();
  const order = createMemo(() => stripOrder(project));
  const [openEffect, setOpenEffect] = createSignal<OpenEffect | null>(null);
  const sampleRate = createStreamRate();
  const showEffect = (node: Id, anchor: HTMLElement) => setOpenEffect({ node, anchor: anchor.getBoundingClientRect() });

  return (
    <div class="flex h-full min-h-[560px] overflow-x-auto" aria-label={t().mixer.title}>
      <div class="flex flex-1 items-stretch gap-px">
        <For each={order()} fallback={<p class="p-4 text-sm text-muted">{t().mixer.empty}</p>}>
          {(id) => <ChannelStrip node={id} sampleRate={sampleRate()} onOpenEffect={showEffect} />}
        </For>
      </div>
      <div class="sticky right-0 flex shrink-0 border-l border-field-line shadow-[-4px_0_8px_rgba(0,0,0,0.15)]">
        <MasterStrip onOpenEffect={showEffect} />
        <MasterMeters />
      </div>
      <Show when={openEffect()}>{(open) => <EffectPopover open={open()} onClose={() => setOpenEffect(null)} />}</Show>
    </div>
  );
}
