// Track mixer strip in its header: volume and pan, M/S and level meter; all of it lives in the graph's source node.
import { Show } from "solid-js";
import type { GraphNode } from "../api/types";
import { isSource } from "../graph/graphRules";
import { faderToVolume, formatDb, SILENT_LEVEL, volumeToFader } from "../mixer/mixer";
import { t } from "../i18n";
import { useSession } from "../session";
import { Knob } from "../common/Knob";
import { Meter } from "../common/Meter";
import { ParamMenu } from "../common/ParamMenu";
import { MuteSolo } from "../common/MuteSolo";
import { PanKnob } from "../common/PanKnob";
import { sticky } from "../common/sticky";

const KNOB_SIZE = 34;
const METER_HEIGHT = 22;
/** The volume knob turns by fader position (same curve as the graph fader); default is 0 dB. */
const VOLUME_RANGE = { min: 0, max: 1, default: volumeToFader(1) } as const;

export function TrackStrip(props: { source: GraphNode }) {
  const session = useSession();
  const { graph } = session;
  const strip = () => (isSource(props.source.kind) ? props.source.kind : undefined);
  const lastStrip = sticky(strip);
  const level = () => session.mixer.levels[props.source.id] ?? SILENT_LEVEL;

  return (
    <Show when={strip()}>
      <div class="flex items-center gap-1.5">
        <div class="flex min-w-0 flex-1 items-start justify-around">
          <ParamMenu target={{ type: "nodeVolume", node: props.source.id }}>
            <Knob info={{ ...VOLUME_RANGE, label: t().mixer.volumeShort }} value={volumeToFader(lastStrip()!.volume)} size={KNOB_SIZE}
              format={(position) => formatDb(faderToVolume(position))} onChange={(position) => void graph.setVolume(props.source.id, faderToVolume(position))} />
          </ParamMenu>
          <PanKnob node={props.source.id} pan={lastStrip()!.pan} size={KNOB_SIZE} />
        </div>
        <MuteSolo node={props.source.id} mute={lastStrip()!.mute} solo={lastStrip()!.solo} class="!h-6 !min-w-6 !px-0 text-[10px]" />
        <Meter level={level()} height={METER_HEIGHT} />
      </div>
    </Show>
  );
}
