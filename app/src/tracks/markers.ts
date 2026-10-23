import { PPQ, type Id } from "../api/types";
import { neighbourMarker, nextMarkerName } from "./markerRules";
import { barStart } from "../common/tempoMap";
import type { Core } from "../core/core";

/** Jumping "to previous" during playback skips a marker passed less than an eighth note ago. */
const JUMP_SLACK_TICKS = PPQ / 2;

export interface Markers {
  /** Marker at `tick` (default name "Section N"). */
  add(tick: number, name?: string): Promise<void>;
  /** "+ Marker": a marker at the start of the playhead's bar. */
  addAtPlayhead(): Promise<void>;
  move(id: Id, tick: number): Promise<void>;
  rename(id: Id, name: string): Promise<void>;
  remove(id: Id): Promise<void>;
  /** Moves the playhead to the previous (`-1`) or next (`1`) marker; does nothing if there is none. */
  jump(direction: -1 | 1): Promise<void>;
}

export function createMarkers(core: Core): Markers {
  const { api } = core;
  const add: Markers["add"] = (tick, name) => core.calls.exec(api.markers.add(tick, name ?? nextMarkerName(core.copy.project.markers)));
  return {
    add,
    addAtPlayhead: () => add(barStart(core.copy.project.tempoMap, core.playback.tick().positionTicks)),
    async move(id, tick) {
      core.copy.patchProject((p) => {
        const marker = p.markers.find((m) => m.id === id);
        if (marker) marker.tick = tick;
        p.markers.sort((a, b) => a.tick - b.tick);
      });
      await core.calls.exec(api.markers.edit(id, { tick }));
    },
    rename: (id, name) => core.calls.exec(api.markers.edit(id, { name })),
    remove: (id) => core.calls.exec(api.markers.remove(id)),
    async jump(direction) {
      const target = neighbourMarker(core.copy.project.markers, core.playback.tick().positionTicks, direction, JUMP_SLACK_TICKS);
      if (target) await core.calls.exec(api.transport.seek(target.tick));
    },
  };
}
