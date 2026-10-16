// Clip selection and group move: pure functions, no DOM or state.
import type { Clip, ClipEdit, Id } from "../api/types";
import { clipEnd } from "./clips";
import { laneOf, type Lanes } from "./trackRows";
import { isAudioTrack } from "./tracks";

/** Clip position and length: what dragging and resizing change. */
export type Placement = Pick<Clip, "track" | "startTick" | "lengthTicks">;

/** Position of one clip in a group (what the draft draws and what goes to `commitMany`). */
export type GroupPlacement = Placement & { id: Id };

/** Marquee rectangle in ticks and track rows (inclusive bounds, any order). */
export interface Area {
  fromTick: number;
  toTick: number;
  fromRow: number;
  toRow: number;
}

/** Shift/Cmd click: toggles the clip in the selection. */
export function toggleId(ids: Id[], id: Id): Id[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}

/** Union without duplicates, preserving order. */
export function unionIds(a: Id[], b: Id[]): Id[] {
  return [...new Set([...a, ...b])];
}

/** Clips touched by the marquee. */
export function clipsInArea(clips: Clip[], tracks: Lanes, area: Area): Id[] {
  const [t0, t1] = [Math.min(area.fromTick, area.toTick), Math.max(area.fromTick, area.toTick)];
  const [r0, r1] = [Math.min(area.fromRow, area.toRow), Math.max(area.fromRow, area.toRow)];
  return clips
    .filter((c) => {
      const row = laneOf(tracks, c.track);
      return row >= r0 && row <= r1 && c.startTick <= t1 && clipEnd(c) > t0;
    })
    .map((c) => c.id);
}

export const groupStart = (clips: Clip[]): number => Math.min(...clips.map((c) => c.startTick));

export const groupEnd = (clips: Clip[]): number => Math.max(...clips.map(clipEnd));

/**
 * Moves the group by the same delta in ticks and rows. The tick delta never moves the leftmost clip below zero.
 * The row delta shrinks toward zero until no clip leaves the tracks or lands on a track of the wrong kind
 * (note clips only on instrument tracks, audio clips only on audio tracks).
 */
export function moveGroup(group: Clip[], tracks: Lanes, deltaTick: number, deltaRows: number): GroupPlacement[] {
  if (group.length === 0) return [];
  const tick = Math.max(deltaTick, -groupStart(group));
  const rowOf = (c: Clip) => laneOf(tracks, c.track);
  const fits = (shift: number) =>
    group.every((c) => {
      const from = tracks[rowOf(c)];
      const to = tracks[rowOf(c) + shift];
      // A folder row (`null`) does not accept clips: moving through it jumps to the track beyond.
      return from != null && to != null && isAudioTrack(from) === isAudioTrack(to);
    });
  let shift = deltaRows;
  while (shift !== 0 && !fits(shift)) shift -= Math.sign(shift);
  return group.map((c) => ({
    id: c.id,
    track: shift === 0 ? c.track : tracks[rowOf(c) + shift]!.id,
    startTick: c.startTick + tick,
    lengthTicks: c.lengthTicks,
  }));
}

/** Whether the edit changes the clip. A missing clip counts as changed (let the backend report that it is gone). */
export function isEditChanged(clips: Clip[], id: Id, edit: ClipEdit): boolean {
  const clip = clips.find((c) => c.id === id);
  return !clip || (Object.keys(edit) as (keyof ClipEdit)[]).some((key) => edit[key] !== clip[key]);
}
