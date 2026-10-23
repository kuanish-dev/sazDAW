import type { AutoPoint, AutomationClip, AutomationClipEdit, AutomationWriteMode, Id, ParamTarget, ShapeRequest } from "../api/types";
import { laneOfTarget } from "./automationRules";
import { copySelected, pasteAt, type PointClip } from "./automationEdit";
import type { Core } from "../core/core";
import type { Selection } from "../selection/selection";
import type { Views } from "../shell/views";
import type { ParamTouch } from "./paramTouch";

export interface Automation {
  /** Selects the lane row of `target` on the Tracks tab, creating the lane if there is none yet. */
  showFor(target: ParamTarget): Promise<void>;
  /** Selects lane row `lane` and, optionally, its clip `clip` (its pattern opens in the clip panel); `null` clears. */
  select(lane: Id | null, clip?: Id | null): void;
  removeLane(id: Id): Promise<void>;
  /** A clip as a flat line at the parameter's current value; it becomes selected. */
  addClip(lane: Id, startTick: number, lengthTicks: number): Promise<void>;
  editClip(lane: Id, clip: Id, edit: AutomationClipEdit): Promise<void>;
  /** A copy right after the clip; the copy becomes selected. */
  duplicateClip(lane: Id, clip: Id): Promise<void>;
  removeClip(lane: Id, clip: Id): Promise<void>;
  /** Replaces the clip's pattern points (ticks from the clip start, `0…loopTicks`). */
  setPoints(lane: Id, clip: Id, points: AutoPoint[]): Promise<void>;
  /** Replaces the pattern points in the request range with a shape (one undo step). */
  drawShape(lane: Id, clip: Id, shape: ShapeRequest): Promise<void>;
  /** ⌘C in a pattern: remembers the selected points (one clipboard for every pattern while the app is open); none selected — keeps the old copy. */
  copyPoints(points: AutoPoint[], selected: Set<number>): void;
  /** ⌘V: `points` with the remembered ones pasted at pattern tick `at` (they come back selected); `null` — nothing copied yet. */
  pastedPoints(points: AutoPoint[], at: number): { points: AutoPoint[]; selected: Set<number> } | null;
  setWriteMode(mode: AutomationWriteMode): Promise<void>;
  /** The mouse button went up anywhere: controls touched for live writing are released. */
  releaseTouches(): Promise<void>;
}

export function createAutomation(core: Core, selection: Selection, views: Views, touch: ParamTouch): Automation {
  const { api } = core;
  let clipboard: PointClip | null = null;
  /**
   * Shows a change of a clip at once, before the backend replies: otherwise a dropped clip or a moved point is drawn
   * at its old place for a frame (the gesture draft is gone, the new project has not arrived yet).
   */
  const show = (lane: Id, clip: Id, change: (found: AutomationClip) => void) =>
    core.copy.patchProject((p) => {
      const found = p.automation.find((l) => l.id === lane)?.clips.find((c) => c.id === clip);
      if (found) change(found);
    });
  return {
    async showFor(target) {
      const id = laneOfTarget(core.copy.project, target)?.id ?? (await core.calls.run(api.automation.addLane(target)));
      if (id === undefined) return;
      selection.selectAutomation({ lane: id, clip: null });
      views.showTab("tracks");
    },
    select: (lane, clip = null) => selection.selectAutomation(lane === null ? null : { lane, clip }),
    removeLane: (id) => core.calls.exec(api.automation.removeLane(id)),
    async addClip(lane, startTick, lengthTicks) {
      const clip = await core.calls.run(api.automation.addClip(lane, startTick, lengthTicks));
      if (clip !== undefined) selection.selectAutomation({ lane, clip });
    },
    editClip(lane, clip, edit) {
      show(lane, clip, (found) => Object.assign(found, edit));
      return core.calls.exec(api.automation.editClip(lane, clip, edit));
    },
    async duplicateClip(lane, clip) {
      const copy = await core.calls.run(api.automation.duplicateClip(lane, clip));
      if (copy !== undefined) selection.selectAutomation({ lane, clip: copy });
    },
    removeClip: (lane, clip) => core.calls.exec(api.automation.removeClip(lane, clip)),
    setPoints(lane, clip, points) {
      show(lane, clip, (found) => {
        found.points = points;
      });
      return core.calls.exec(api.automation.setPoints(lane, clip, points));
    },
    drawShape: (lane, clip, shape) => core.calls.exec(api.automation.drawShape(lane, clip, shape)),
    copyPoints(points, selected) {
      clipboard = copySelected(points, selected) ?? clipboard;
    },
    pastedPoints: (points, at) => (clipboard ? pasteAt(points, clipboard, at) : null),
    setWriteMode: (mode) => core.calls.exec(api.automation.setWriteMode(mode)),
    releaseTouches: () => touch.releaseAll(),
  };
}
