import { Show } from "solid-js";
import type { GraphNode } from "../api/types";
import { instrumentInfo } from "../common/catalog";
import { hasVolume, isSource, nodeLabel } from "./graphRules";
import { formatDb, SILENT_LEVEL } from "../mixer/mixer";
import { t } from "../i18n";
import { Fader } from "../common/Fader";
import { ParamMenu } from "../common/ParamMenu";
import { Meter } from "../common/Meter";
import { MuteSolo } from "../common/MuteSolo";
import { useProjectCopy } from "../core/roles";
import { useGraph } from "./role";
import { useMixer } from "../mixer/role";

const STRIP_HEIGHT = 64;

export function NodeStrip(props: { node: GraphNode }) {
  const { catalog, project } = useProjectCopy();
  const graph = useGraph();
  const mixer = useMixer();
  const level = () => mixer.levels[props.node.id] ?? SILENT_LEVEL;
  const volume = () => (hasVolume(props.node.kind) ? props.node.kind.volume : 0);
  const source = () => (isSource(props.node.kind) ? props.node.kind : null);
  /** Caption under a source's name: the track instrument's name or "Audio track". */
  const caption = () => {
    const s = source();
    const instrument = s && project.tracks.find((track) => track.id === s.track)?.instrument;
    if (!instrument) return t().graph.audioTrack;
    return instrumentInfo(catalog(), instrument.kind)?.label ?? instrument.kind;
  };

  return (
    <>
      <Show when={source()}>
        <div class="truncate px-3 pt-1 text-[11px] text-muted">{caption()}</div>
      </Show>
      <div class="flex items-stretch justify-center gap-3 pt-2">
        <ParamMenu target={{ type: "nodeVolume", node: props.node.id }}>
          <Fader volume={volume()} height={STRIP_HEIGHT} label={t().mixer.volumeOf(nodeLabel(props.node))} onChange={(v) => void graph.setVolume(props.node.id, v)} />
        </ParamMenu>
        <Meter level={level()} height={STRIP_HEIGHT} />
        <div class="flex flex-col items-center justify-end gap-1">
          <Show when={source()}>
            {(s) => (
              <div class="flex gap-1">
                <MuteSolo node={props.node.id} mute={s().mute} solo={s().solo} class="!h-5 !min-w-0 !px-1.5 text-[10px]" />
              </div>
            )}
          </Show>
          <span class="font-mono text-[11px]">{formatDb(volume())}</span>
        </div>
      </div>
    </>
  );
}
