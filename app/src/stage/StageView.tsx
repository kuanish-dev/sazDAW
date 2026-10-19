// The Stage page: drag the instruments around a room and hear them move. The room from above (left/right, near/far),
// the side view (near/far, height), the channel list and the shared room's settings. Where each channel stands is the
// project's (`StageControl`, through `session.stage`); the geometry is `stage/stageRules.ts`. While a dot is being
// dragged the pictures show the dragged place itself, so a project re-read that is a move behind cannot pull it back.
import { Show, createMemo, createSignal } from "solid-js";
import { DEFAULT_STAGE_PLACE, type Id, type Listening, type ParamTarget, type Room } from "../api/types";
import { formatPan } from "../mixer/mixer";
import { hearing, roomMeasures, roomShape, type Spot, type StageChannel, stageChannels } from "./stageRules";
import { t } from "../i18n";
import { useSession } from "../session";
import { IconAdd, IconHeadphones, IconSpeaker } from "../common/icons";
import { ParamMenu } from "../common/ParamMenu";
import { ToolButton, Toolbar, ToolbarDivider } from "../common/Toolbar";
import { trackColorOf } from "../common/trackColors";
import type { StageScene } from "./scene";
import { StageList } from "./StageList";
import { StageRoom } from "./StageRoom";
import { StageSide } from "./StageSide";

/** A bus has no track color. */
const BUS_COLOR = "var(--color-muted)";

export function StageView() {
  const session = useSession();
  const stage = session.stage;
  const channels = createMemo(() => stageChannels(session.project));
  const room = () => session.project.room;
  const [selected, setSelected] = createSignal<Id | null>(null);
  const [dragging, setDragging] = createSignal<{ node: Id; spot: Spot } | null>(null);

  const channelOf = (node: Id): StageChannel | undefined => channels().find((c) => c.node === node);
  const spotOf = (node: Id): Spot | undefined => {
    const dragged = dragging();
    return dragged?.node === node ? dragged.spot : channelOf(node)?.spot;
  };
  const colorOf = (node: Id): string => {
    const track = channelOf(node)?.track;
    return (track != null ? trackColorOf(session.project.tracks, track) : null) ?? BUS_COLOR;
  };
  const placed = createMemo(() => channels().flatMap((c) => (c.spot ? [c.node] : [])), undefined, {
    equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]),
  });
  const offStage = () => channels().filter((c) => !c.spot);

  const scene: StageScene = {
    shape: createMemo(() => roomShape(room().size)),
    listening: () => room().listening,
    placed,
    spotOf,
    nameOf: (node) => channelOf(node)?.name ?? "",
    colorOf,
    selected,
    select: setSelected,
    drag(node, spot) {
      setDragging({ node, spot });
      stage.place(node, spot);
    },
    endDrag: () => setDragging(null),
    place: (node, spot) => stage.place(node, spot),
    nudge(node, change) {
      const spot = spotOf(node);
      if (spot) stage.place(node, { ...spot, ...change(spot) });
    },
  };
  const setRoom = (change: Partial<Room>) => stage.setRoom({ ...room(), ...change });
  /** One channel comes on the stage at its own pan, a few steps in front of the listener. */
  const placeOne = (channel: StageChannel) => {
    stage.place(channel.node, { pan: channel.pan, ...DEFAULT_STAGE_PLACE });
    setSelected(channel.node);
  };
  const listenOn = (listening: Listening) => setRoom({ listening });

  return (
    <div class="flex h-full min-h-[420px] flex-col" aria-label={t().stage.title}>
      <Toolbar label={t().stage.title}>
        <label class="flex items-center gap-2 text-xs" title={t().stage.sizeHint}>
          <span class="text-muted">{t().stage.size}</span>
          <input type="range" class="w-28" min="0" max="1" step="0.01" value={room().size} onInput={(e) => setRoom({ size: Number(e.currentTarget.value) })} />
          <span class="w-24 font-mono text-[11px] text-muted">{t().stage.measures(roomMeasures(room()))}</span>
        </label>
        <label class="flex items-center gap-2 text-xs" title={t().stage.reverbHint}>
          <span class="text-muted">{t().stage.reverb}</span>
          <input type="range" class="w-28" min="0" max="1" step="0.01" value={room().reverb} onInput={(e) => setRoom({ reverb: Number(e.currentTarget.value) })} />
        </label>
        <ToolbarDivider />
        <ToolButton
          icon={<IconSpeaker />} label={t().stage.speakers} title={t().stage.speakersHint} text
          pressed={room().listening === "speakers"} onClick={() => listenOn("speakers")}
        />
        <ToolButton
          icon={<IconHeadphones />} label={t().stage.headphones} title={t().stage.headphonesHint} text
          pressed={room().listening === "headphones"} onClick={() => listenOn("headphones")}
        />
        <ToolbarDivider />
        <ToolButton
          icon={<IconAdd />} label={t().stage.placeAll} title={t().stage.placeAllHint} text disabled={offStage().length === 0}
          onClick={() => void stage.placeAll()}
        />
        <span class="flex-1" />
        <span class="hidden truncate text-[11px] text-muted xl:inline">{t().stage.hint}</span>
      </Toolbar>
      <div class="flex min-h-0 flex-1">
        <div class="min-w-0 flex-1 p-3">
          <StageRoom scene={scene} />
        </div>
        <aside class="flex w-80 shrink-0 flex-col border-l border-line bg-panel">
          <div class="section-header px-3 py-1">{t().stage.sideView}</div>
          <div class="p-2">
            <StageSide scene={scene} />
          </div>
          <Readout scene={scene} />
          <StageList
            channels={channels()} colorOf={colorOf} selected={selected()} onSelect={setSelected} onPlace={placeOne}
            onTakeOff={(node) => void stage.takeOff(node)}
          />
        </aside>
      </div>
    </div>
  );
}

/** What the listener hears from the selected channel: how far, how loud directly, from which direction; and its three
 *  coordinates, each with the parameter menu (right-click: automation lane, MIDI controller). */
function Readout(props: { scene: StageScene }) {
  const heard = () => {
    const node = props.scene.selected();
    const spot = node === null ? undefined : props.scene.spotOf(node);
    return node !== null && spot ? { node, spot, name: props.scene.nameOf(node), ...hearing(props.scene.shape(), spot) } : undefined;
  };
  /** Left/right of straight ahead, or beyond ±90° left/right of straight behind. */
  const side = (degrees: number) => {
    const behind = Math.abs(degrees) > 90;
    const rounded = Math.round(behind ? 180 - Math.abs(degrees) : Math.abs(degrees));
    if (rounded === 0) return behind ? t().stage.behind : t().stage.ahead;
    if (behind) return degrees < 0 ? t().stage.behindLeft(String(rounded)) : t().stage.behindRight(String(rounded));
    return degrees < 0 ? t().stage.toLeft(String(rounded)) : t().stage.toRight(String(rounded));
  };
  const up = (degrees: number) => {
    const rounded = Math.round(Math.abs(degrees));
    if (rounded === 0) return t().stage.earHeight;
    return degrees < 0 ? t().stage.below(String(rounded)) : t().stage.above(String(rounded));
  };
  return (
    <Show when={heard()}>
      {(h) => (
        <div class="border-y border-line px-3 py-2 text-xs">
          <div class="truncate font-medium">{h().name}</div>
          <div class="text-muted">
            {[t().stage.away(h().distance.toFixed(1)), t().stage.direct(h().levelDb.toFixed(1)), side(h().azimuth), up(h().elevation)].join(" · ")}
          </div>
          <div class="mt-1.5 flex gap-1" title={t().stage.paramHint}>
            <Coordinate target={{ type: "nodePan", node: h().node }} label={t().stage.leftRight} value={formatPan(h().spot.pan)} />
            <Coordinate target={{ type: "stageDepth", node: h().node }} label={t().stage.depth} value={`${Math.round(h().spot.depth * 100)}%`} />
            <Coordinate target={{ type: "stageHeight", node: h().node }} label={t().stage.height} value={`${Math.round(h().spot.height * 100)}%`} />
          </div>
        </div>
      )}
    </Show>
  );
}

/** One coordinate of the selected channel, with its parameter menu. */
function Coordinate(props: { target: ParamTarget; label: string; value: string }) {
  return (
    <ParamMenu target={props.target}>
      <span class="flex-1 rounded border border-line bg-bg px-1.5 py-0.5 text-[11px]">
        <span class="text-muted">{props.label}</span> <span class="font-mono">{props.value}</span>
      </span>
    </ParamMenu>
  );
}
