// Panel right of the graph canvas: settings of the selected node — effect parameters, fader, pan, M/S and the channel
// utility of a source or bus, name and outgoing connections, "+ Effect" after a source or bus and "+ Effect on master"
// (`GraphView` does the insert).
import { For, Match, Show, Switch } from "solid-js";
import type { GraphNode } from "../../api/types";
import { kindChoices } from "../../domain/catalog";
import { hasStrip, hasVolume, nodeLabel, outgoing } from "../../domain/graph";
import { formatDb, SILENT_LEVEL } from "../../domain/mixer";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { ChannelUtility } from "../common/ChannelUtility";
import { Fader } from "../common/Fader";
import { IconArrowRight, IconClose } from "../common/icons";
import { ParamMenu } from "../common/ParamMenu";
import { Meter } from "../common/Meter";
import { MuteSolo } from "../common/MuteSolo";
import { PanKnob } from "../common/PanKnob";
import { PickSelect } from "../common/PickSelect";
import { createStreamRate } from "../common/streamRate";
import { EffectPanel } from "../effects/EffectPanel";

const PAN_KNOB_SIZE = 40;
const UTILITY_KNOB_SIZE = 36;

export function Inspector(props: { node: GraphNode | null; onInsertEffect: (kind: string) => void }) {
  return (
    <aside class="flex w-64 shrink-0 flex-col overflow-y-auto border-l border-line bg-panel" aria-label={t().graph.inspector.title}>
      <Show when={props.node} fallback={<p class="p-3 text-sm text-muted">{t().graph.inspector.pickNode}</p>}>
        {(node) => (
          <>
            <h2 class="section-header truncate border-b border-line">{nodeLabel(node())}</h2>
            <Switch>
              <Match when={node().kind.type === "effect"}><EffectPanel node={node()} /></Match>
              <Match when={node().kind.type === "master"}>
                <MixPanel node={node()} />
                <InsertPanel label={t().graph.inspector.insertOnMaster} title={t().graph.inspector.insertOnMasterHint} onPick={props.onInsertEffect} />
              </Match>
              <Match when={true}>
                <MixPanel node={node()} />
                <InsertPanel label={t().graph.inspector.insert} title={t().graph.inspector.insertHint} onPick={props.onInsertEffect} />
                <LinksPanel node={node()} />
              </Match>
            </Switch>
          </>
        )}
      </Show>
    </aside>
  );
}

/** Source or bus: fader, pan, mute/solo and the channel utility; the master has only a fader. */
function MixPanel(props: { node: GraphNode }) {
  const session = useSession();
  const { graph } = session.actions;
  const sampleRate = createStreamRate();
  const volume = () => (hasVolume(props.node.kind) ? props.node.kind.volume : 0);
  const strip = () => (hasStrip(props.node.kind) ? props.node.kind : null);
  const level = () => session.levels[props.node.id] ?? SILENT_LEVEL;
  return (
    <div class="flex flex-col items-center gap-3 p-3">
      <Show when={strip()}>{(s) => <PanKnob node={props.node.id} pan={s().pan} size={PAN_KNOB_SIZE} />}</Show>
      <div class="flex items-stretch gap-3">
        <ParamMenu target={{ type: "nodeVolume", node: props.node.id }}>
          <Fader volume={volume()} label={t().mixer.volumeOf(nodeLabel(props.node))} onChange={(v) => void graph.setVolume(props.node.id, v)} />
        </ParamMenu>
        <Meter level={level()} />
      </div>
      <div class="font-mono text-xs">{formatDb(volume())}</div>
      <Show when={strip()}>
        {(s) => (
          <>
            <div class="flex w-full gap-2">
              <MuteSolo node={props.node.id} mute={s().mute} solo={s().solo} class="flex-1" />
            </div>
            <div class="w-full">
              <ChannelUtility node={props.node.id} strip={s()} sampleRate={sampleRate()} knobSize={UTILITY_KNOB_SIZE} />
            </div>
          </>
        )}
      </Show>
    </div>
  );
}

function InsertPanel(props: { label: string; title: string; onPick: (kind: string) => void }) {
  const session = useSession();
  const effects = () => kindChoices(session.catalog()?.effects ?? []);
  return (
    <div class="px-3">
      <PickSelect class="w-full text-xs" options={effects()} placeholder={props.label} title={props.title} onPick={props.onPick} />
    </div>
  );
}

/** Node name and the list of outgoing connections with a remove button. */
function LinksPanel(props: { node: GraphNode }) {
  const session = useSession();
  const { graph } = session.actions;
  const edges = () => outgoing(session.project.graph, props.node.id);
  const nameOf = (id: number) => {
    const target = session.project.graph.nodes.find((n) => n.id === id);
    return target ? nodeLabel(target) : `#${id}`;
  };
  return (
    <div class="flex flex-col gap-2 p-3">
      <input
        class="field w-full"
        value={props.node.name}
        aria-label={t().graph.inspector.name}
        onChange={(e) => e.currentTarget.value.trim() && void graph.renameNode(props.node.id, e.currentTarget.value.trim())}
      />
      <h3 class="label">{t().graph.inspector.outputsTo}</h3>
      <For each={edges()} fallback={<p class="text-sm text-muted">{t().graph.inspector.noOutputs}</p>}>
        {(edge) => (
          <div class="-mx-1 flex h-6 items-center gap-2 rounded px-1 text-xs hover:bg-hover">
            <IconArrowRight size={12} class="text-muted" />
            <span class="min-w-0 flex-1 truncate"><Show when={edge.sidechain}><span class="mr-1 text-[10px] font-semibold text-muted">SC</span></Show>{nameOf(edge.to)}</span>
            <Show when={edge.preFader}><span class="text-muted" title={t().graph.inspector.preFaderHint}>{t().graph.inspector.preFaderShort}</span></Show>
            <span class="font-mono text-muted">{Math.round(edge.level * 100)}%</span>
            <button class="btn btn-icon !h-5 !w-5 !min-w-5 hover:!text-danger" aria-label={t().graph.edge.remove} title={t().graph.edge.remove} onClick={() => void graph.disconnect(edge.from, edge.to, edge.sidechain)}><IconClose size={12} /></button>
          </div>
        )}
      </For>
    </div>
  );
}
