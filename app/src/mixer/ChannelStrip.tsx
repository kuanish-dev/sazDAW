import { Show, createMemo, createSignal } from "solid-js";
import type { GraphNode, Id, Track } from "../api/types";
import { instrumentInfo } from "../common/catalog";
import { hasStrip, hasVolume, isSource, nodeLabel } from "../graph/graphRules";
import { formatDb, SILENT_LEVEL } from "./mixer";
import { masterModel, stripModel } from "./mixerStrips";
import { t } from "../i18n";
import { ChannelUtility } from "../common/ChannelUtility";
import { Fader } from "../common/Fader";
import { ParamMenu } from "../common/ParamMenu";
import { InlineRename } from "../common/InlineRename";
import { Meter } from "../common/Meter";
import { MuteSolo } from "../common/MuteSolo";
import { PanKnob } from "../common/PanKnob";
import { trackColorOf } from "../common/trackColors";
import { MasterVisualizer } from "./MasterVisualizer";
import { InsertList, OutputSelect, SendList } from "./StripRouting";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "../graph/role";
import { useMixer } from "./role";
import { useSelection } from "../selection/role";
import { useTracks } from "../tracks/role";

const FADER_HEIGHT = 180;
const PAN_KNOB_SIZE = 34;
const UTILITY_KNOB_SIZE = 28;

type OpenEffect = (node: Id, anchor: HTMLElement) => void;

export function ChannelStrip(props: { node: Id; sampleRate: number; onOpenEffect: OpenEffect }) {
  const { project } = useProjectCopy();
  const selection = useSelection();
  const model = createMemo(() => stripModel(project.graph, props.node));
  const track = () => {
    const kind = model()?.node.kind;
    return kind && isSource(kind) ? project.tracks.find((track) => track.id === kind.track) : undefined;
  };
  const selected = () => track() !== undefined && selection.track() === track()!.id;
  /** The node's channel (a track source or a bus always has one; kept as a guard). */
  const strip = () => {
    const kind = model()?.node.kind;
    return kind && hasStrip(kind) ? kind : null;
  };
  /** Same color as the track on the timeline; a bus or the master has none. */
  const color = () => {
    const shown = track();
    return shown ? trackColorOf(project.tracks, shown.id) : null;
  };

  return (
    <Show when={model()}>
      {(m) => (
        <section
          class="flex w-36 shrink-0 flex-col gap-2 border-r border-line bg-panel p-2"
          classList={{ "!bg-select": selected() }}
          aria-label={t().mixer.strip(nodeLabel(m().node))}
          onPointerDown={() => track() && selection.selectTrack(track()!.id)}
        >
          <Show when={color()}>{(c) => <div class="-mx-2 -mt-2 h-1" style={{ background: c() }} />}</Show>
          <StripTitle node={m().node} track={track()} />
          <InsertList model={m()} onOpenEffect={props.onOpenEffect} />
          <SendList model={m()} />
          <div class="flex flex-col gap-0.5">
            <span class="label">{t().mixer.output}</span>
            <OutputSelect model={m()} />
          </div>
          <div class="mt-auto flex flex-col gap-2">
            <Show when={strip()}>
              {(s) => <ChannelUtility node={props.node} strip={s()} sampleRate={props.sampleRate} knobSize={UTILITY_KNOB_SIZE} />}
            </Show>
            <StripMix node={m().node} />
          </div>
        </section>
      )}
    </Show>
  );
}

/** Master: the live visualizer, inserts before it and a fader; the mixer pins it to the right edge with the meters. */
export function MasterStrip(props: { onOpenEffect: OpenEffect }) {
  const { project } = useProjectCopy();
  const model = createMemo(() => masterModel(project.graph));
  return (
    <Show when={model()}>
      {(m) => (
        <section class="flex w-36 shrink-0 flex-col gap-2 bg-panel p-2" aria-label={t().mixer.masterStrip}>
          <StripTitle node={m().node} track={undefined} />
          <MasterVisualizer />
          <InsertList model={m()} onOpenEffect={props.onOpenEffect} />
          <StripMix node={m().node} />
        </section>
      )}
    </Show>
  );
}

/** Strip name and kind. A track name is changed by the tracks action, a bus name by the graph action; the master can't be renamed. */
function StripTitle(props: { node: GraphNode; track: Track | undefined }) {
  const { catalog } = useProjectCopy();
  const graph = useGraph();
  const tracks = useTracks();
  const [renaming, setRenaming] = createSignal(false);
  const name = () => props.track?.name ?? nodeLabel(props.node);
  const caption = () => {
    if (props.node.kind.type === "bus") return t().mixer.bus;
    if (props.node.kind.type === "master") return t().mixer.master;
    const instrument = props.track?.instrument;
    return instrument ? (instrumentInfo(catalog(), instrument.kind)?.label ?? instrument.kind) : t().graph.audioTrack;
  };
  const rename = (next: string) => {
    if (props.track) void tracks.rename(props.track.id, next);
    else void graph.renameNode(props.node.id, next);
  };
  return (
    <div class="min-w-0">
      <InlineRename name={name()} editing={renaming()} class="!h-6 w-full !px-1 text-sm" onRename={rename} onClose={() => setRenaming(false)}>
        <div
          class="truncate text-sm font-semibold" title={t().mixer.renameHint(name())}
          onDblClick={() => props.node.kind.type !== "master" && setRenaming(true)}
        >
          {name()}
        </div>
      </InlineRename>
      <div class="truncate text-[11px] text-muted">{caption()}</div>
    </div>
  );
}

/** Strip bottom: pan, fader with meter, dB and M/S. The meter reads its own node's level — only it re-renders. */
function StripMix(props: { node: GraphNode }) {
  const graph = useGraph();
  const mixer = useMixer();
  const volume = () => (hasVolume(props.node.kind) ? props.node.kind.volume : 0);
  const strip = () => (hasStrip(props.node.kind) ? props.node.kind : null);
  const level = () => mixer.levels[props.node.id] ?? SILENT_LEVEL;
  return (
    <div class="mt-auto flex flex-col items-center gap-2">
      <Show when={strip()} fallback={<div style={{ height: `${PAN_KNOB_SIZE + 24}px` }} />}>
        {(s) => <PanKnob node={props.node.id} pan={s().pan} size={PAN_KNOB_SIZE} />}
      </Show>
      <div class="flex items-stretch gap-2">
        <ParamMenu target={{ type: "nodeVolume", node: props.node.id }}>
          <Fader volume={volume()} height={FADER_HEIGHT} label={t().mixer.volumeOf(nodeLabel(props.node))} onChange={(v) => void graph.setVolume(props.node.id, v)} />
        </ParamMenu>
        <Meter level={level()} height={FADER_HEIGHT} />
      </div>
      <div class="font-mono text-[11px]">{formatDb(volume())}</div>
      <div class="flex w-full gap-1" style={{ visibility: strip() ? "visible" : "hidden" }}>
        <MuteSolo node={props.node.id} mute={strip()?.mute ?? false} solo={strip()?.solo ?? false} class="!h-6 flex-1 !min-w-0 text-xs" />
      </div>
    </div>
  );
}
