// Dropping a sample on the timeline: on dragover computes the insertion spot (a clip "ghost"), on drop asks the action to create a clip.
import { createSignal, type Accessor } from "solid-js";
import type { Clip, Id, TempoMap, Track } from "../api/types";
import { planDrop, type SampleSource } from "./dropSample";
import { type RowLayout, rowAt } from "./rowLayout";
import { barLengthAt, snapDownInBars } from "../common/tempoMap";
import type { Lanes } from "./trackRows";
import { draggedSample, registerSampleDrop } from "../common/dragSample";
import { spanRect, type PixelRect } from "./timelineLayout";

/** Ghost under the cursor: a clip on a track (`clip`) or replacing the track's instrument with the sound (`instrument`, whole row highlighted). */
export type DropGhost = PixelRect & { mode: "clip" | "instrument" };

/**
 * Where to draw the ghost and where the clip will land for the cursor at canvas point (x, y); `null` if it can't be dropped here.
 * If the sound's length is unknown (no `lengthTicks`), the ghost spans one bar at the drop point.
 */
export function ghostAt(
  x: number, y: number,
  o: { tracks: Lanes; clips: Clip[]; snap: number; tempoMap: TempoMap; pxPerTick: number; layout: RowLayout; lengthTicks?: number; width: number },
): (DropGhost & { track: Id; startTick: number }) | null {
  const row = rowAt(y, o.layout);
  if (row === null) return null;
  const startTick = Math.max(0, snapDownInBars(o.tempoMap, x / o.pxPerTick, o.snap));
  const lengthTicks = o.lengthTicks ?? barLengthAt(o.tempoMap, startTick);
  const plan = planDrop(o.tracks.filter((t): t is Track => t !== null), o.clips, o.tracks[row]?.id ?? null, startTick, lengthTicks);
  if (!plan) return null;
  // A clip ghost sits exactly where the clip will; an instrument drop lights the whole row.
  const rect = spanRect(startTick, lengthTicks, row, o.pxPerTick, o.layout);
  return {
    track: plan.track,
    startTick,
    mode: plan.instrument ? "instrument" : "clip",
    ...(plan.instrument ? { ...rect, x: 0, w: o.width } : rect),
  };
}

interface TimelineDropDeps {
  /** Timeline rows: a track, or `null` for a folder row (nothing can be dropped there). */
  tracks: Accessor<Lanes>;
  clips: Accessor<Clip[]>;
  snap: Accessor<number>;
  tempoMap: Accessor<TempoMap>;
  pxPerTick: Accessor<number>;
  layout: Accessor<RowLayout>;
  /** Accepts a drop on the track under the cursor: passes the track and tick to the action. */
  drop: (source: SampleSource, track: Id, tick: number) => void;
}

/** Sound drop target for the timeline canvas wrapper: ghost on move, action on release. */
export function createTimelineDrop(deps: TimelineDropDeps) {
  const [ghost, setGhost] = createSignal<DropGhost | null>(null);

  const locate = (el: HTMLElement, x: number, y: number) => {
    const box = el.getBoundingClientRect();
    return ghostAt(x - box.left, y - box.top, {
      tracks: deps.tracks(), clips: deps.clips(), snap: deps.snap(), tempoMap: deps.tempoMap(), pxPerTick: deps.pxPerTick(), layout: deps.layout(),
      lengthTicks: draggedSample()?.lengthTicks ?? undefined, width: box.width,
    });
  };

  return {
    ghost,
    /** A sound is being dragged: "new track" zones are shown below the last track. */
    active: () => draggedSample() !== null,
    /** Makes the wrapper a drop target; called from `ref`. */
    attach(el: HTMLElement) {
      registerSampleDrop(el, {
        over: (x, y) => setGhost(locate(el, x, y)),
        leave: () => setGhost(null),
        drop(source, x, y) {
          const place = locate(el, x, y);
          setGhost(null);
          if (place) deps.drop(source, place.track, place.startTick);
        },
      });
    },
  };
}
