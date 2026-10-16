// Song structure actions: section markers on the timeline ruler and jumping between them. Errors are notifications.
import { PPQ, type Id } from "../api/types";
import { neighbourMarker, nextMarkerName } from "../domain/markers";
import { barStart } from "../domain/tempoMap";
import type { Backend, Calls, PlaybackPosition, ProjectCopy } from "./context";

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

export function createMarkersActions(ctx: Backend & Calls & ProjectCopy & PlaybackPosition): MarkersActions {
  const { api } = ctx;
  const add: MarkersActions["add"] = (tick, name) => ctx.exec(api.markers.add(tick, name ?? nextMarkerName(ctx.project.markers)));
  return {
    add,
    addAtPlayhead: () => add(barStart(ctx.project.tempoMap, ctx.tick().positionTicks)),
    async move(id, tick) {
      // The flag stays where it was dropped without waiting for the backend.
      ctx.patchProject((p) => {
        const marker = p.markers.find((m) => m.id === id);
        if (marker) marker.tick = tick;
        p.markers.sort((a, b) => a.tick - b.tick);
      });
      await ctx.exec(api.markers.edit(id, { tick }));
    },
    rename: (id, name) => ctx.exec(api.markers.edit(id, { name })),
    remove: (id) => ctx.exec(api.markers.remove(id)),
    async jump(direction) {
      const target = neighbourMarker(ctx.project.markers, ctx.tick().positionTicks, direction, JUMP_SLACK_TICKS);
      if (target) await ctx.exec(api.transport.seek(target.tick));
    },
  };
}
