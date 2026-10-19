// Timeline clip actions: creation, group edits, the clip clipboard, splitting and trimming, tempo warping, pitch,
// fades, normalize and reverse of audio clips, takes and comping. The loop region is the transport's (`transport/transport.ts`).
import type { Clip, ClipEdit, FadeShape, Id } from "../api/types";
import { groupEnd, isEditChanged } from "./clipSelection";
import type { FadeEdge } from "./fades";
import { snapNearestInBars } from "../common/tempoMap";
import { BAR_GRID } from "../common/time";
import { roundSemitones } from "./warp";
import type { Core } from "../core/core";
import type { Views } from "../shell/views";

export interface ClipsActions {
  /** Empty note clip on an instrument track (one bar by default); selects the clip and its track. */
  addNote(track: Id, startTick: number, lengthTicks?: number): Promise<Clip | undefined>;
  edit(id: Id, edit: ClipEdit): Promise<Clip | undefined>;
  /** Finishes dragging a group: a single undo step; unchanged clips are skipped. */
  commitMany(changes: { id: Id; edit: ClipEdit }[]): Promise<void>;
  removeMany(ids: Id[]): Promise<void>;
  /** Copies on the same tracks, with the leftmost placed at `atTick`. Selects the new clips. */
  copy(ids: Id[], atTick: number): Promise<Clip[] | undefined>;
  /** Ctrl+C: remember the clips (by id) in an internal clipboard. */
  remember(ids: Id[]): void;
  /** Ctrl+V: paste the remembered clips at the bar of the playhead (occupied space is an error toast). */
  paste(): Promise<void>;
  /** Ctrl+D: copies right after the group's right edge. */
  duplicateMany(ids: Id[]): Promise<void>;
  /** Splits a clip at a tick; returns the right half. */
  split(id: Id, tick: number): Promise<Clip | undefined>;
  remove(id: Id): Promise<void>;
  /**
   * Moves the clip's left edge to `trimmed.startTick` keeping its end; the content stays in place. `trimmed` is the clip
   * as the drag drew it (`tracks/clipRules` `trimmedClip`, which needs the sound's sample rate): it is shown until the backend replies.
   */
  trimStart(trimmed: Clip): Promise<void>;
  /** Fade shape of one edge of an audio clip. */
  setFadeShape(id: Id, edge: FadeEdge, shape: FadeShape): Promise<void>;
  /** Sets audio clips' gain so their loudest sample reaches −0.3 dBFS (non-destructive, one undo step). */
  normalize(ids: Id[]): Promise<void>;
  /** Plays the clip backwards from a reversed copy of its sample (needs a saved project). */
  reverse(id: Id): Promise<void>;
  /** "Warp to tempo" for an audio clip: turning it on guesses the source tempo (file name, length); off plays the audio at its own tempo. */
  setWarp(id: Id, on: boolean): Promise<void>;
  /** Source tempo of a warped clip (the clip length is recomputed so it still holds the same piece of audio). */
  setSourceBpm(id: Id, bpm: number): Promise<void>;
  /** Audio clip pitch in semitones (to hundredths); the length does not change. */
  setPitch(id: Id, semitones: number): Promise<void>;
  /** The whole audio clip plays take `take` (comping is cleared; clip position and length stay). */
  setActiveTake(id: Id, take: Id): Promise<void>;
  /** Removes a take from the clip; the sample stays in "Samples". */
  removeTake(id: Id, take: Id): Promise<void>;
  /** Comping: range `[from, to)` (ticks from clip start) plays take `take`; the rest is unchanged. */
  chooseTakeRange(id: Id, from: number, to: number, take: Id): Promise<void>;
}

export function createClipsActions(core: Core, views: Views): ClipsActions {
  const { api } = core;
  let clipboard: Id[] = [];
  /** Clips by id that still exist in the project (the clipboard must survive deletions). */
  const existing = (ids: Id[]) => ids.filter((id) => core.project.clips.some((c) => c.id === id));

  /**
   * Shows edits at once, before the backend replies: otherwise a dropped clip is drawn at its old place for a frame
   * (the gesture draft is gone, the new project has not arrived yet). A warp edit changes the length too, which only
   * the backend computes, so it waits for the reply.
   */
  const showEdits = (changes: { id: Id; edit: ClipEdit }[]) =>
    core.patchProject((p) => {
      for (const { id, edit } of changes) {
        const clip = p.clips.find((c) => c.id === id);
        if (clip && edit.warp === undefined) Object.assign(clip, edit);
      }
    });

  const copy: ClipsActions["copy"] = async (ids, atTick) => {
    if (ids.length === 0) return undefined;
    const copies = await core.run(api.clips.copy(ids, atTick));
    if (copies) await views.showCreated({ clips: copies.map((c) => c.id) });
    return copies;
  };

  return {
    async addNote(track, startTick, lengthTicks) {
      const clip = await core.run(api.clips.addNote({ track, startTick, lengthTicks }));
      if (clip) await views.showCreated({ clips: [clip.id] });
      return clip;
    },
    edit(id, edit) {
      showEdits([{ id, edit }]);
      return core.run(api.clips.edit(id, edit));
    },
    async commitMany(changes) {
      const real = changes.filter((c) => isEditChanged(core.project.clips, c.id, c.edit));
      if (real.length === 0) return;
      showEdits(real);
      await core.exec(api.clips.editMany(real));
    },
    async removeMany(ids) {
      if (ids.length > 0) await core.exec(api.clips.removeMany(ids));
    },
    copy,
    remember: (ids) => {
      clipboard = [...ids];
    },
    async paste() {
      const ids = existing(clipboard);
      if (ids.length > 0) await copy(ids, snapNearestInBars(core.project.tempoMap, core.tick().positionTicks, BAR_GRID));
    },
    async duplicateMany(ids) {
      const group = core.project.clips.filter((c) => ids.includes(c.id));
      if (group.length > 0) await copy(ids, groupEnd(group));
    },
    split: (id, tick) => core.run(api.clips.split(id, tick)),
    remove: (id) => core.exec(api.clips.remove(id)),
    trimStart(trimmed) {
      core.patchProject((p) => {
        const index = p.clips.findIndex((c) => c.id === trimmed.id);
        if (index >= 0) p.clips[index] = trimmed;
      });
      return core.exec(api.clips.trimStart(trimmed.id, trimmed.startTick));
    },
    setFadeShape: (id, edge, shape) => core.exec(api.clips.edit(id, edge === "in" ? { fadeInShape: shape } : { fadeOutShape: shape })),
    async normalize(ids) {
      if (ids.length > 0) await core.exec(api.clips.normalize(ids));
    },
    reverse: (id) => core.exec(api.clips.reverse(id)),
    setWarp: (id, on) => core.exec(api.clips.edit(id, { warp: on ? {} : null })),
    setSourceBpm: (id, bpm) => core.exec(api.clips.edit(id, { warp: { sourceBpm: bpm } })),
    setPitch: (id, semitones) => core.exec(api.clips.edit(id, { pitchSemitones: roundSemitones(semitones) })),
    setActiveTake: (id, take) => core.exec(api.clips.setActiveTake(id, take)),
    removeTake: (id, take) => core.exec(api.clips.removeTake(id, take)),
    chooseTakeRange: (id, from, to, take) => core.exec(api.clips.chooseTakeRange(id, from, to, take)),
  };
}
