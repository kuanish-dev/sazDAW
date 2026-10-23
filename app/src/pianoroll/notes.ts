import { createSignal } from "solid-js";
import { MIDI_FILTER } from "../api/dialogs";
import {
  STEP_TICKS, type Clip, type ControllerKind, type ControllerPoint, type GrooveTemplate, type Id, type NewNote, type Note, type NoteContent, type Scale, type Variation,
} from "../api/types";
import { sameKind } from "./controllers";
import { joinPath } from "../common/format";
import { clipNotes } from "./noteClip";
import { DEFAULT_VELOCITY } from "../common/pitch";
import { copyNotes, pasteNotes, type NoteChange, type NoteClipboard } from "./noteSelection";
import { notesAtStep } from "./steps";
import { clipById, trackById } from "../tracks/trackRules";
import { t } from "../i18n";
import type { Core } from "../core/core";
import { homeDir } from "../files/paths";

export interface Notes {
  /** Step input: add or remove the step's notes in the clip at pitch `pitch` (defaults to the track's pitch). */
  setStep(clip: Id, step: number, want: "add" | "remove", pitch?: number): Promise<void>;

  addNote(clip: Id, note: NewNote): Promise<Note | undefined>;
  removeNote(clip: Id, id: Id): Promise<void>;
  addNotes(clip: Id, notes: NewNote[]): Promise<Note[] | undefined>;
  /** The changed notes show at once (a finished drag), before the backend replies. */
  updateNotes(clip: Id, changes: NoteChange[]): Promise<Note[] | undefined>;
  removeNotes(clip: Id, ids: Id[]): Promise<void>;
  clear(clip: Id): Promise<void>;
  /** Manual length of the looped content (turns off auto length). */
  setLoopTicks(clip: Id, loopTicks: number): Promise<void>;

  /** Pull starts (with `ends`, ends too) toward the `grid`-tick grid by `strength` 0…1. */
  quantize(clip: Id, ids: Id[] | undefined, grid: number, strength?: number, ends?: boolean): Promise<Note[] | undefined>;
  /** Randomly shift starts by ±`timingTicks` and velocity by ±`velocity`. */
  humanize(clip: Id, ids: Id[] | undefined, timingTicks: number, velocity: number): Promise<Note[] | undefined>;
  transpose(clip: Id, ids: Id[] | undefined, semitones: number): Promise<Note[] | undefined>;
  /** To the nearest degree of the scale on root `root` (0 = C); at equal distance — down. */
  fitToScale(clip: Id, ids: Id[] | undefined, root: number, scale: Scale): Promise<Note[] | undefined>;
  /** Each note extends to the start of the group's next note. */
  legato(clip: Id, ids: Id[] | undefined): Promise<Note[] | undefined>;
  /** Groove from a template with amount `amount` 0…1 (edits the notes; undo reverts it). */
  groove(clip: Id, ids: Id[] | undefined, template: GrooveTemplate, amount: number): Promise<Note[] | undefined>;
  /** Groove extracted from another note clip `sourceClip`. */
  grooveFromClip(clip: Id, ids: Id[] | undefined, sourceClip: Id, amount: number): Promise<Note[] | undefined>;
  /** Pattern variation: remove/add notes, neighbouring degrees, rhythm, velocity; one seed — one result. */
  vary(clip: Id, ids: Id[] | undefined, variation: Variation): Promise<Note[] | undefined>;
  /** A copy of the clip right after it with varied notes (one undo step); the answer is the new clip. */
  varyToNewClip(clip: Id, ids: Id[] | undefined, variation: Variation): Promise<Clip | undefined>;
  /** A whole controller lane (empty removes the lane); one undo step. The lane shows at once, before the backend replies. */
  setControllerLane(clip: Id, kind: ControllerKind, points: ControllerPoint[]): Promise<void>;

  /** Ctrl+C: remember the notes (one clipboard for all clips while the app is open); shows a "copied" toast. */
  copy(notes: Note[]): void;
  /** Whether there is something to paste (reactive: the Paste button tracks it). */
  canPaste(): boolean;
  /** Ctrl+V: clipboard notes into the clip, the group starting at `atTick`; returns the created notes. */
  paste(clip: Id, atTick: number): Promise<Note[] | undefined>;

  /** Export the note clip to .mid via a save dialog; cancelling the dialog does nothing. */
  exportClip(clip: Id): Promise<void>;
  /** Import .mid into the clip via an open dialog; returns the number of notes. */
  importInto(clip: Id): Promise<number | undefined>;
}

export function createNotes(core: Core): Notes {
  const { api } = core;
  const [clipboard, setClipboard] = createSignal<NoteClipboard | null>(null);
  const trackOfClip = (clip: Id) => trackById(core.copy.project, clipById(core.copy.project, clip)?.track ?? null);
  /**
   * Shows an edit of the clip's notes at once: otherwise a dropped note or a drawn lane is drawn at its old place for a
   * frame (the gesture draft is gone, the new project has not arrived yet).
   */
  const show = (clip: Id, change: (content: NoteContent) => void) =>
    core.copy.patchProject((p) => {
      const content = p.clips.find((c) => c.id === clip)?.content;
      if (content?.type === "notes") change(content);
    });

  return {
    async setStep(clipId, step, want, pitch) {
      const clip = clipById(core.copy.project, clipId);
      const track = trackOfClip(clipId);
      if (clip?.content.type !== "notes" || !track) return;
      const height = pitch ?? track.defaultPitch;
      const existing = notesAtStep(clipNotes(clip), step, height);
      if (want === "remove") {
        if (existing.length > 0) await core.calls.exec(api.notes.removeMany(clip.id, existing.map((n) => n.id)));
      } else if (existing.length === 0) {
        await core.calls.exec(api.notes.add(clip.id, { pitch: height, startTick: step * STEP_TICKS, lengthTicks: STEP_TICKS, velocity: DEFAULT_VELOCITY }));
      }
    },

    addNote: (clip, note) => core.calls.run(api.notes.add(clip, note)),
    removeNote: (clip, id) => core.calls.exec(api.notes.remove(clip, id)),
    addNotes: (clip, notes) => core.calls.run(api.notes.addMany(clip, notes)),
    updateNotes(clip, changes) {
      show(clip, (content) => {
        for (const { id, note } of changes) {
          const found = content.notes.find((n) => n.id === id);
          if (found) Object.assign(found, note);
        }
      });
      return core.calls.run(api.notes.updateMany(clip, changes));
    },
    removeNotes: (clip, ids) => core.calls.exec(api.notes.removeMany(clip, ids)),
    clear: (clip) => core.calls.exec(api.notes.clear(clip)),
    setLoopTicks: (clip, loopTicks) => core.calls.exec(api.notes.setLoopTicks(clip, loopTicks)),

    quantize: (clip, ids, grid, strength = 1, ends = false) => core.calls.run(api.notes.quantize(clip, ids, grid, strength, ends)),
    humanize: (clip, ids, timingTicks, velocity) => core.calls.run(api.notes.humanize(clip, ids, timingTicks, velocity)),
    transpose: (clip, ids, semitones) => core.calls.run(api.notes.transpose(clip, ids, semitones)),
    fitToScale: (clip, ids, root, scale) => core.calls.run(api.notes.fitToScale(clip, ids, root, scale)),
    legato: (clip, ids) => core.calls.run(api.notes.legato(clip, ids)),
    groove: (clip, ids, template, amount) => core.calls.run(api.notes.groove(clip, ids, template, amount)),
    grooveFromClip: (clip, ids, sourceClip, amount) => core.calls.run(api.notes.grooveFromClip(clip, ids, sourceClip, amount)),
    vary: (clip, ids, variation) => core.calls.run(api.notes.vary(clip, ids, variation)),
    async varyToNewClip(clip, ids, variation) {
      const created = await core.calls.run(api.notes.varyToNewClip(clip, ids, variation));
      if (created) core.notifications.toast("info", t().pianoRoll.variationToNewClip);
      return created;
    },
    setControllerLane(clip, kind, points) {
      show(clip, (content) => {
        const others = (content.controllers ?? []).filter((lane) => !sameKind(lane.kind, kind));
        content.controllers = points.length === 0 ? others : [...others, { kind, points }];
      });
      return core.calls.exec(api.notes.setControllerLane(clip, kind, points));
    },

    copy(notes) {
      const copied = copyNotes(notes);
      if (!copied) return;
      setClipboard(copied);
      core.notifications.toast("info", t().pianoRoll.notesCopied(notes.length));
    },
    canPaste: () => clipboard() !== null,
    async paste(clip, atTick) {
      const copied = clipboard();
      return copied ? core.calls.run(api.notes.addMany(clip, pasteNotes(copied, atTick))) : undefined;
    },

    async exportClip(clipId) {
      const name = trackOfClip(clipId)?.name ?? "clip";
      const path = await core.dialogs.saveFile({
        title: t().pianoRoll.exportMidiTitle, filters: [MIDI_FILTER], defaultPath: joinPath(await homeDir(api), `${name}.mid`),
      });
      if (path && (await core.calls.attempt(api.midi.exportClip(clipId, path)))) core.notifications.toast("info", t().pianoRoll.midiWritten);
    },
    async importInto(clipId) {
      const track = trackOfClip(clipId);
      if (!track) return undefined;
      const path = await core.dialogs.openFile({ title: t().pianoRoll.importMidiTitle(track.name), filters: [MIDI_FILTER], defaultPath: await homeDir(api) });
      if (!path) return undefined;
      const count = await core.calls.run(api.midi.importInto(clipId, path));
      if (count !== undefined) core.notifications.toast("info", t().pianoRoll.notesImported(count));
      return count;
    },
  };
}
