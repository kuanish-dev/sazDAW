// Clip group methods: edit, delete and copy several clips as one undo step.
import type { Clip, ClipEdit, Id } from "../../api/types";
import { t } from "../../i18n";
import { applyEdit, checkLimit, checkNoOverlaps, checkRange } from "../clipRules";
import { fail } from "../errors";
import type { MockWorld } from "../world";
import type { HandlerSet } from "./types";

const unique = (ids: Id[]) => [...new Set(ids)];

export function clipGroupsHandlers(w: MockWorld): HandlerSet {
  const h: HandlerSet = {};

  // All edits apply to the draft and overlaps are checked on the result, so a group can move edge to edge.
  h["clips.editMany"] = ({ changes }: { changes: { id: Id; edit: ClipEdit }[] }) =>
    changes.length === 0 ? [] : w.edit(t().mockEdits.history.editClips, (p) => {
      const ids = unique(changes.map((c) => c.id));
      const result = changes.map((c) => {
        const clip = w.clip(p, c.id);
        applyEdit(w, p, clip, c.edit);
        return clip;
      });
      checkNoOverlaps(p, ids);
      return result;
    });

  h["clips.removeMany"] = ({ ids }: { ids: Id[] }) =>
    ids.length === 0 ? undefined : w.edit(t().mockEdits.history.removeClips, (p) => {
      for (const id of ids) w.clip(p, id);
      const gone = new Set(ids);
      p.clips = p.clips.filter((c) => !gone.has(c.id));
    });

  h["clips.copy"] = ({ ids, atTick }: { ids: Id[]; atTick: number }) =>
    w.edit(t().mockEdits.history.copyClips, (p) => {
      if (ids.length === 0) throw fail("CL-118");
      const originals = unique(ids).map((id) => w.clip(p, id));
      checkLimit(p, originals.length);
      const shift = atTick - Math.min(...originals.map((c) => c.startTick));
      const copies = originals.map((c): Clip => {
        const copy = w.clone(c);
        copy.id = w.id(p);
        copy.startTick += shift;
        checkRange(copy.startTick, copy.lengthTicks);
        return copy;
      });
      p.clips.push(...copies);
      checkNoOverlaps(p, copies.map((c) => c.id));
      return copies;
    });

  return h;
}
