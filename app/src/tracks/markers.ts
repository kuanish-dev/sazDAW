// Song structure actions: section markers on the timeline ruler and jumping between them. Errors are notifications.
import { PPQ, type Id } from "../api/types";
import { neighbourMarker, nextMarkerName } from "./markerRules";
import { barStart } from "../common/tempoMap";
import type { Core } from "../core/core";

/** Jumping "to previous" during playback skips a marker passed less than an eighth note ago. */
const JUMP_SLACK_TICKS = PPQ / 2;

export interface MarkersActions {
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

export function createMarkersActions(core: Core): MarkersActions {
  const { api } = core;
  const add: MarkersActions["add"] = (tick, name) => core.exec(api.markers.add(tick, name ?? nextMarkerName(core.project.markers)));
  return {
    add,
    addAtPlayhead: () => add(barStart(core.project.tempoMap, core.tick().positionTicks)),
    async move(id, tick) {
      // The flag stays where it was dropped without waiting for the backend.
      core.patchProject((p) => {
        const marker = p.markers.find((m) => m.id === id);
        if (marker) marker.tick = tick;
        p.markers.sort((a, b) => a.tick - b.tick);
      });
      await core.exec(api.markers.edit(id, { tick }));
    },
    rename: (id, name) => core.exec(api.markers.edit(id, { name })),
    remove: (id) => core.exec(api.markers.remove(id)),
    async jump(direction) {
      const target = neighbourMarker(core.project.markers, core.tick().positionTicks, direction, JUMP_SLACK_TICKS);
      if (target) await core.exec(api.transport.seek(target.tick));
    },
  };
}
