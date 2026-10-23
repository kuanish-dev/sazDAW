import type { AutomationClip, AutomationLane, Id, TempoMap } from "../api/types";
import type { Automation } from "../automation/automation";
import { freeSpanAt, overlapsAutoClips } from "../automation/automationRules";
import { hitSpan, minClipLength } from "./clipRules";
import { type RowLayout, rowAt } from "./rowLayout";
import { barLengthAt, snapDownInBars, snapNearestInBars } from "../common/tempoMap";
import type { TrackRow } from "./trackRows";
import { capture, localPoint } from "../common/pointer";
import { HANDLE_PX, tickAtX } from "./timelineLayout";

/** An automation clip being dragged, as it would land. */
export interface LaneDraft {
  lane: Id;
  clip: AutomationClip;
}

type Drag = { mode: "move"; lane: AutomationLane; clip: AutomationClip; grab: number } | { mode: "resize"; lane: AutomationLane; clip: AutomationClip };

export interface LanePointerDeps {
  surface: () => HTMLElement;
  rows: () => readonly TrackRow[];
  snap: () => number;
  tempoMap: () => TempoMap;
  pxPerTick: () => number;
  layout: () => RowLayout;
  actions: Automation;
  draft: () => LaneDraft | null;
  setDraft: (draft: LaneDraft | null) => void;
}

export function createLanePointer(deps: LanePointerDeps) {
  let drag: Drag | null = null;

  /** The lane row under the mouse and the tick there; `null` — not a lane row. */
  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.surface(), event);
    const rows = deps.rows();
    const index = rowAt(y, deps.layout());
    const row = index === null ? undefined : rows[index];
    return row?.type === "lane" ? { lane: row.lane, tick: tickAtX(x, deps.pxPerTick()) } : null;
  };
  const hit = (lane: AutomationLane, tick: number) => {
    for (const clip of lane.clips) {
      const zone = hitSpan(clip.startTick, clip.lengthTicks, tick, HANDLE_PX / deps.pxPerTick());
      if (zone) return { clip, zone };
    }
    return null;
  };
  const snapped = (tick: number) => snapNearestInBars(deps.tempoMap(), tick, deps.snap());

  return {
    onDown(event: PointerEvent): boolean {
      const at = locate(event);
      if (!at) return false;
      if (event.button !== 0) return true;
      const found = hit(at.lane, at.tick);
      deps.actions.select(at.lane.id, found?.clip.id ?? null);
      if (!found) return true;
      capture(deps.surface(), event);
      drag = found.zone === "resize"
        ? { mode: "resize", lane: at.lane, clip: found.clip }
        : { mode: "move", lane: at.lane, clip: found.clip, grab: at.tick - found.clip.startTick };
      return true;
    },

    onMove(event: PointerEvent): boolean {
      const tick = tickAtX(localPoint(deps.surface(), event).x, deps.pxPerTick());
      if (!drag) {
        const at = locate(event);
        if (!at) return false;
        const zone = hit(at.lane, at.tick)?.zone;
        deps.surface().style.cursor = zone === "resize" ? "ew-resize" : zone ? "grab" : "default";
        return true;
      }
      const { lane, clip } = drag;
      const next = drag.mode === "move"
        ? { ...clip, startTick: Math.max(0, snapped(tick - drag.grab)) }
        : { ...clip, lengthTicks: Math.max(minClipLength(clip.startTick, deps.snap(), deps.tempoMap()), snapped(tick) - clip.startTick) };
      if (!overlapsAutoClips(lane, next.startTick, next.lengthTicks, clip.id)) deps.setDraft({ lane: lane.id, clip: next });
      return true;
    },

    onUp(): boolean {
      const finished = drag;
      const draft = deps.draft();
      drag = null;
      deps.setDraft(null);
      if (!finished) return false;
      if (!draft) return true;
      const { lane, clip } = finished;
      if (finished.mode === "move" && draft.clip.startTick !== clip.startTick) {
        void deps.actions.editClip(lane.id, clip.id, { startTick: draft.clip.startTick });
      } else if (finished.mode === "resize" && draft.clip.lengthTicks !== clip.lengthTicks) {
        void deps.actions.editClip(lane.id, clip.id, { lengthTicks: draft.clip.lengthTicks });
      }
      return true;
    },

    /** Double-click on empty space of a lane row: a one-bar clip there (shortened to fit before the next clip). */
    onDouble(event: MouseEvent): boolean {
      const at = locate(event);
      if (!at) return false;
      if (hit(at.lane, at.tick)) return true;
      const { from, to } = freeSpanAt(at.lane, snapDownInBars(deps.tempoMap(), at.tick, deps.snap()));
      const length = Math.min(barLengthAt(deps.tempoMap(), from), to - from);
      if (length > 0) void deps.actions.addClip(at.lane.id, from, length);
      return true;
    },

    /** Right click on a clip deletes it. */
    onContext(event: MouseEvent): boolean {
      const at = locate(event);
      if (!at) return false;
      event.preventDefault();
      const found = hit(at.lane, at.tick);
      if (found) void deps.actions.removeClip(at.lane.id, found.clip.id);
      return true;
    },
  };
}
