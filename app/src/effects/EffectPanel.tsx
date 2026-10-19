// Effect settings panel (graph inspector and the mixer's effect popover): bypass, presets, plugin window, the sidechain
// source picker for effects that read a key, the effect's live graph (`EffectGraph`), and generic controls for every
// parameter (the bare `sidechain` switch is replaced by the picker).
import { For, Show } from "solid-js";
import type { GraphNode } from "../api/types";
import { effectInfo, paramValue } from "../common/catalog";
import { isPluginKind } from "../plugins/pluginRules";
import { t } from "../i18n";
import { IconPower } from "../common/icons";
import { ParamControl } from "../common/ParamControl";
import { PresetBar } from "../common/PresetBar";
import { EffectGraph } from "./EffectGraph";
import { SidechainPicker } from "./SidechainPicker";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "../graph/role";
import { usePlugins } from "../plugins/role";

/** The catalog param that makes an effect read its sidechain key (`set_sidechain` in the backend). */
const SIDECHAIN_PARAM = "sidechain";

export function EffectPanel(props: { node: GraphNode }) {
  const { catalog } = useProjectCopy();
  const graph = useGraph();
  const plugins = usePlugins();
const { editor } = plugins;
  const kind = () => props.node.kind as Extract<GraphNode["kind"], { type: "effect" }>;
  const info = () => effectInfo(catalog(), kind().effect);
  const params = () => info()?.params ?? [];
  const hasSidechain = () => params().some((p) => p.name === SIDECHAIN_PARAM);
  const knobs = () => params().filter((p) => p.name !== SIDECHAIN_PARAM);
  const setParam = (name: string, value: number) => void graph.setEffectParam(props.node.id, name, value);
  return (
    <div class="flex flex-col gap-3 p-3">
      <div class="flex items-center justify-between gap-2 text-sm">
        <span class="min-w-0 truncate">{info()?.label ?? kind().effect}</span>
        <button
          class="btn shrink-0 text-xs" classList={{ "btn-on": !kind().bypass, "text-muted": kind().bypass }} aria-pressed={!kind().bypass}
          title={t().graph.inspector.bypassHint} onClick={() => void graph.setEffectBypass(props.node.id, !kind().bypass)}
        >
          <IconPower size={14} />{kind().bypass ? t().graph.inspector.bypassed : t().graph.inspector.active}
        </button>
      </div>
      <PresetBar target={{ type: "effect", node: props.node.id }} kind={kind().effect} />
      <Show when={isPluginKind(kind().effect)}>
        <button class="btn text-xs" title={t().common.pluginWindowHint} onClick={() => void editor.open({ type: "effect", node: props.node.id })}>
          {t().common.pluginWindow}
        </button>
      </Show>
      <Show when={hasSidechain()}>
        <SidechainPicker effect={props.node.id} />
      </Show>
      <EffectGraph node={props.node} />
      <div class="grid grid-cols-[repeat(auto-fill,minmax(64px,1fr))] gap-x-2 gap-y-4">
        <For each={knobs()}>
          {(p) => <ParamControl info={p} value={paramValue(kind().params, p)} target={{ type: "effectParam", node: props.node.id, param: p.name }} onChange={(v) => setParam(p.name, v)} />}
        </For>
      </div>
    </div>
  );
}
