// Notes of a note clip: add, edit, delete, replace, content length, musical edits of a note group
// (rules in `pianoroll/noteTransforms.ts`, `groove.ts`, `variation.ts`, copies of the backend) and controller lanes. Each is one undo step.
import {
  MAX_CLIP_LOOP_TICKS, type GrooveTemplate, type Id, type NewNote, type Note, type NoteContent, type Project, type ScaleKey, type Variation,
} from "../../api/types";
import {
  checkLoopTicks, contentOf, createNote, fitLength, limitCount, noteLimit, replaceNotes, setControllerLane, setLoopTicks, validateNote,
} from "../noteOps";
import { clipBarTicks } from "../../pianoroll/noteClip";
import {
  MAX_HUMANIZE_TICKS, MAX_HUMANIZE_VELOCITY, fitGroupToScale, humanizeGroup, isScale, legatoGroup, quantizeGroup, transposeGroup,
} from "../../pianoroll/noteTransforms";
import { applyGroove, extractGroove, MAX_GROOVE_SHIFT_TICKS, templateTable } from "../../pianoroll/groove";
import { varyGroup } from "../../pianoroll/variation";
import { duplicateClip } from "../clipRules";
import { fail } from "../errors";
import { t } from "../../i18n";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

export function notesHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};

  /** A clip notes edit: content comes from the project copy; the length is fitted afterwards. */
  const editNotes = <R>(label: string, clip: Id, change: (content: NoteContent) => R): R =>
    w.edit(label, (p: Project) => {
      const target = w.clip(p, clip);
      const content = contentOf(target);
      const result = change(content);
      fitLength(content, clipBarTicks(p.tempoMap, target));
      return result;
    });

  const replaceAt = (content: NoteContent, id: Id, note: NewNote): Note => {
    const index = content.notes.findIndex((n) => n.id === id);
    if (index < 0) throw fail("NT-001", { id });
    const updated: Note = { id, ...note };
    content.notes[index] = updated;
    return updated;
  };

  h["notes.add"] = ({ clip, note }) => editNotes(t().mockEdits.history.addNote, clip, (content) => {
    validateNote(note, noteLimit(content));
    limitCount(content.notes.length + 1);
    const created = createNote(content, note);
    content.notes.push(created);
    return created;
  });
  h["notes.update"] = ({ clip, id, note }) => editNotes(t().mockEdits.history.updateNote, clip, (content) => {
    validateNote(note, noteLimit(content));
    return replaceAt(content, id, note);
  });
  h["notes.remove"] = ({ clip, id }) => editNotes(t().mockEdits.history.removeNote, clip, (content) => {
    if (!content.notes.some((n) => n.id === id)) throw fail("NT-001", { id });
    content.notes = content.notes.filter((n) => n.id !== id);
  });
  h["notes.clear"] = ({ clip }) => editNotes(t().mockEdits.history.clearClip, clip, (content) => { content.notes = []; });
  h["notes.addMany"] = ({ clip, notes }) => editNotes(t().mockEdits.history.addNotes, clip, (content) => {
    const limit = noteLimit(content);
    limitCount(content.notes.length + notes.length);
    const created = (notes as NewNote[]).map((n) => { validateNote(n, limit); return createNote(content, n); });
    content.notes.push(...created);
    return created;
  });
  h["notes.updateMany"] = ({ clip, changes }) => editNotes(t().mockEdits.history.updateNotes, clip, (content) => {
    const limit = noteLimit(content);
    return (changes as { id: Id; note: NewNote }[]).map(({ id, note }) => {
      validateNote(note, limit);
      return replaceAt(content, id, note);
    });
  });
  h["notes.removeMany"] = ({ clip, ids }) => editNotes(t().mockEdits.history.removeNotes, clip, (content) => {
    for (const id of ids as Id[]) if (!content.notes.some((n) => n.id === id)) throw fail("NT-001", { id });
    content.notes = content.notes.filter((n) => !(ids as Id[]).includes(n.id));
  });
  h["notes.replace"] = ({ clip, notes }) => editNotes(t().mockEdits.history.replaceNotes, clip, (content) => replaceNotes(content, notes));
  h["notes.setLoopTicks"] = ({ clip, loopTicks }) => w.edit(t().mockEdits.history.clipContentLength, (p) => {
    checkLoopTicks(loopTicks);
    const target = w.clip(p, clip);
    setLoopTicks(contentOf(target), loopTicks);
    // The phase cannot exceed the new length.
    target.offsetTicks %= loopTicks;
  });

  /** Notes of group `ids` (none means all clip notes) in content order; an empty list or unknown note is an error. */
  const groupOf = (content: NoteContent, ids: Id[] | undefined): Note[] => {
    if (ids !== undefined && ids.length === 0) throw fail("NT-111");
    for (const id of ids ?? []) if (!content.notes.some((n) => n.id === id)) throw fail("NT-001", { id });
    return content.notes.filter((n) => ids === undefined || ids.includes(n.id));
  };

  /** A musical group edit: `ids` (none means all notes), group → result; one step, all or nothing. */
  const transform = (label: string, clip: Id, ids: Id[] | undefined, change: (group: Note[], limit: number) => Note[]) =>
    editNotes(label, clip, (content) => {
      const group = groupOf(content, ids);
      const limit = noteLimit(content);
      const changed = change(group, limit);
      for (const n of changed) validateNote(n, limit);
      const byId = new Map(changed.map((n) => [n.id, n]));
      content.notes = content.notes.map((n) => byId.get(n.id) ?? n);
      return changed;
    });
  const inRange = (value: number, min: number, max: number) => Number.isInteger(value) && value >= min && value <= max;
  const checkAmount = (amount: number) => {
    if (!(amount >= 0 && amount <= 1)) throw fail("NT-222", { amount });
  };
  /** Groove template limits (as `daw-contracts`' `GrooveTemplate` checks). */
  const checkTemplate = (template: GrooveTemplate) => {
    if (template.type === "swing" && !inRange(template.percent, 50, 75)) throw fail("NT-223", { percent: template.percent });
    if ((template.type === "laidBack" || template.type === "pushed") && !inRange(template.ticks, 0, MAX_GROOVE_SHIFT_TICKS)) {
      throw fail("NT-224", { ticks: template.ticks, max: MAX_GROOVE_SHIFT_TICKS });
    }
    if (template.type === "accent" && !inRange(template.percent, 100, 200)) throw fail("NT-225", { percent: template.percent });
  };

  h["notes.quantize"] = ({ clip, ids, grid, strength = 1, ends = false }) => {
    if (!inRange(grid, 1, MAX_CLIP_LOOP_TICKS)) throw fail("NT-215", { grid, max: MAX_CLIP_LOOP_TICKS });
    if (!(strength >= 0 && strength <= 1)) throw fail("NT-216", { strength });
    return transform(t().mockEdits.history.quantize, clip, ids, (group, limit) => quantizeGroup(group, grid, strength, ends, limit));
  };
  h["notes.humanize"] = ({ clip, ids, timingTicks, velocity, seed }) => {
    if (!inRange(timingTicks, 0, MAX_HUMANIZE_TICKS)) throw fail("NT-217", { ticks: timingTicks, max: MAX_HUMANIZE_TICKS });
    if (!inRange(velocity, 0, MAX_HUMANIZE_VELOCITY)) throw fail("NT-218", { velocity, max: MAX_HUMANIZE_VELOCITY });
    const actualSeed = seed ?? Math.floor(Math.random() * 2 ** 32);
    return transform(t().mockEdits.history.humanize, clip, ids, (group, limit) => humanizeGroup(group, timingTicks, velocity, actualSeed, limit));
  };
  h["notes.transpose"] = ({ clip, ids, semitones }) => {
    if (!inRange(semitones, -127, 127)) throw fail("NT-219", { semitones });
    return transform(t().mockEdits.history.transpose, clip, ids, (group) => {
      const out = group.find((n) => !inRange(n.pitch + semitones, 0, 127));
      if (out) throw fail("NT-115", { semitones, id: out.id, pitch: out.pitch });
      return transposeGroup(group, semitones);
    });
  };
  h["notes.fitToScale"] = ({ clip, ids, root, scale }) => {
    if (!isScale(scale)) throw fail("RP-101", { method: "notes.fitToScale", details: `unknown variant \`${scale}\`` });
    if (!inRange(root, 0, 11)) throw fail("NT-220", { root });
    return transform(t().mockEdits.history.fitToScale, clip, ids, (group) => fitGroupToScale(group, root, scale));
  };
  h["notes.legato"] = ({ clip, ids }) => transform(t().mockEdits.history.legato, clip, ids, (group) => legatoGroup(group));

  h["notes.groove"] = ({ clip, ids, template, amount = 1 }) => {
    checkTemplate(template);
    checkAmount(amount);
    return transform(t().mockEdits.history.groove, clip, ids, (group, limit) => applyGroove(group, templateTable(template), amount, limit));
  };
  h["notes.grooveFromClip"] = ({ clip, ids, sourceClip, amount = 1 }) => {
    const source = w.clip(w.project, sourceClip);
    const notes = contentOf(source).notes;
    if (notes.length === 0) throw fail("NT-120", { id: sourceClip });
    const table = extractGroove(notes, clipBarTicks(w.project.tempoMap, source));
    checkAmount(amount);
    return transform(t().mockEdits.history.grooveFromClip, clip, ids, (group, limit) => applyGroove(group, table, amount, limit));
  };

  /** A clip notes variation inside an open edit: removed notes drop out, new ones get ids and go to the end. */
  const varyIn = (p: Project, clip: Id, ids: Id[] | undefined, variation: Variation): Note[] => {
    const target = w.clip(p, clip);
    const content = contentOf(target);
    const group = groupOf(content, ids);
    const others = content.notes.filter((n) => !group.includes(n));
    const bar = clipBarTicks(p.tempoMap, target);
    const varied = varyGroup(group, others, variation, variation.seed ?? 0, bar, content.loopTicks);
    const kept = varied.kept.filter((n): n is Note => n !== null);
    limitCount(others.length + kept.length + varied.added.length);
    const limit = noteLimit(content);
    for (const n of kept) validateNote(n, limit);
    const created = varied.added.map((n) => { validateNote(n, limit); return createNote(content, n); });
    const byId = new Map(varied.kept.map((n, i) => [group[i]!.id, n]));
    content.notes = [...content.notes.flatMap((n) => { const after = byId.get(n.id); return after === undefined ? [n] : after ? [after] : []; }), ...created];
    fitLength(content, bar);
    return [...kept, ...created];
  };
  /** Variation parameters from the request: a missing field is 0; no seed means a random one, as in the backend. */
  const variationOf = (params: Record<string, unknown>): Variation => {
    const v: Variation = {
      seed: (params.seed as number | undefined) ?? Math.floor(Math.random() * 2 ** 32),
      density: (params.density as number | undefined) ?? 0,
      pitch: (params.pitch as number | undefined) ?? 0,
      rhythm: (params.rhythm as number | undefined) ?? 0,
      velocity: (params.velocity as number | undefined) ?? 0,
      scale: (params.scale as ScaleKey | null | undefined) ?? undefined,
    };
    const shares = [["NT-116", v.density, -1], ["NT-117", v.pitch, 0], ["NT-118", v.rhythm, 0], ["NT-119", v.velocity, 0]] as const;
    for (const [code, value, min] of shares) if (!(value >= min && value <= 1)) throw fail(code, { value });
    if (v.scale && !inRange(v.scale.root, 0, 11)) throw fail("NT-220", { root: v.scale.root });
    return v;
  };
  h["notes.vary"] = (params) => {
    const variation = variationOf(params);
    return w.edit(t().mockEdits.history.vary, (p) => varyIn(p, params.clip, params.ids, variation));
  };
  h["notes.varyToNewClip"] = (params) => {
    const variation = variationOf(params);
    return w.edit(t().mockEdits.history.varyToNewClip, (p) => {
      const copy = duplicateClip(w, p, params.clip);
      varyIn(p, copy.id, params.ids, variation);
      return w.clip(p, copy.id);
    });
  };

  h["notes.setControllerLane"] = ({ clip, kind, points }) =>
    editNotes(t().mockEdits.history.controllerLane, clip, (content) => { setControllerLane(content, kind, points); });
  return h;
}
