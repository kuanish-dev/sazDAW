import type { Id, ParamTarget, Room } from "../api/types";
import { findNode, hasStrip } from "../graph/graphRules";
import { ensembleSpots, type Spot, stageChannels } from "./stageRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import type { ParamTouch } from "../automation/paramTouch";

export interface Stage {
  /** Puts a channel (a track's source or a bus) on the stage or moves it there; a drag calls it on every move. */
  place(node: Id, spot: Spot): void;
  /** Every channel off the stage comes onto it at once, in an arc in front of the listener: one undo step. */
  placeAll(): Promise<void>;
  /** Off the stage: the channel plays with plain pan again. */
  takeOff(node: Id): Promise<void>;
  /** The shared room (a slider calls it on every move). */
  setRoom(room: Room): void;
}

/** Sends the latest value per key, one call at a time: a value set while a call runs waits for it, and only the newest
 *  waiting one goes next. */
function latestWins<K, V>(send: (key: K, value: V) => Promise<void>): (key: K, value: V) => void {
  const waiting = new Map<K, V>();
  const running = new Set<K>();
  const drain = async (key: K) => {
    running.add(key);
    while (waiting.has(key)) {
      const value = waiting.get(key) as V;
      waiting.delete(key);
      await send(key, value);
    }
    running.delete(key);
  };
  return (key, value) => {
    waiting.set(key, value);
    if (!running.has(key)) void drain(key);
  };
}

export function createStage(core: Core, touch: ParamTouch): Stage {
  const { api } = core;
  const sendPlace = latestWins<Id, Spot>((node, { pan, depth, height }) => core.calls.exec(api.stage.place(node, pan, { depth, height })));
  const sendRoom = latestWins<"room", Room>((_, room) => core.calls.exec(api.stage.setRoom(room)));

  return {
    place(node, spot) {
      const kind = findNode(core.copy.project.graph, node)?.kind;
      if (!kind || !hasStrip(kind)) return;
      if (kind.stage) {
        const moves: [ParamTarget, number][] = [
          [{ type: "nodePan", node }, spot.pan],
          [{ type: "stageDepth", node }, spot.depth],
          [{ type: "stageHeight", node }, spot.height],
        ];
        for (const [target, value] of moves) touch.moved(target, value);
      }
      core.copy.patchProject((p) => {
        const channel = findNode(p.graph, node)?.kind;
        if (!channel || !hasStrip(channel)) return;
        channel.pan = spot.pan;
        channel.stage = { depth: spot.depth, height: spot.height };
      });
      sendPlace(node, spot);
    },
    async placeAll() {
      const off = stageChannels(core.copy.project).filter((channel) => !channel.spot);
      if (off.length === 0) return;
      const mark = await core.calls.run(api.history.checkpoint());
      if (mark === undefined) return;
      const spots = ensembleSpots(off.length);
      for (const [index, channel] of off.entries()) {
        const spot = spots[index];
        if (!spot || !(await core.calls.attempt(api.stage.place(channel.node, spot.pan, { depth: spot.depth, height: spot.height })))) break;
      }
      await core.calls.exec(api.history.squash(mark, t().stage.placeAllStep));
    },
    takeOff: (node) => core.calls.exec(api.stage.takeOff(node)),
    setRoom(room) {
      core.copy.patchProject((p) => {
        p.room = room;
      });
      sendRoom("room", room);
    },
  };
}
