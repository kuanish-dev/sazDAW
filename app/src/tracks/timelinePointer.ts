// Mouse on the timeline: selection (click, Shift/Cmd, marquee), moving and copying a group, resizing and trimming the start,
// audio clip fades and gain (handles on the clip, `timelineLayout`), the audio clip menu, splitting, erasing. The ruler strip
// above the rows has its own pointers (`rulerPointer`, `tempoPointer`, `markerPointer`).
import type { Clip, Id, TempoMap, Track } from "../api/types";
import { laneOf, type Lanes } from "./trackRows";
import type { ClipsActions } from "./clips";
import { clipsInArea, groupStart, moveGroup, toggleId, unionIds } from "./clipSelection";
import { draggedGain, hitClip, overlapsOthers, resizePlacement, trimStartAt, trimmedClip, type ClipHit } from "./clipRules";
import { fadeAt, fadeLengthAt, type FadeEdge } from "./fades";
import { formatDb } from "../mixer/mixer";
import { type RowLayout, clampRow, rowAt } from "./rowLayout";
import { barLengthAt, snapDownInBars, snapNearestInBars } from "../common/tempoMap";
import { t } from "../i18n";
import { menuPoint, type MenuPoint } from "../common/ContextMenu";
import { capture, isAdditive, localPoint } from "../common/pointer";
import {
  HANDLE_PX, audioZoneAt, clipRect, showsGainLine, tickAtX, type AudioZone, type ClipDraft, type PixelRect, type Tool,
} from "./timelineLayout";

type Drag =
  | { mode: "move"; lead: Clip; group: Clip[]; grab: number }
  | { mode: "resize"; clip: Clip }
  | { mode: "trim"; clip: Clip }
  | { mode: "fade"; clip: Clip; edge: FadeEdge }
  | { mode: "gain"; clip: Clip; y: number }
  | { mode: "marquee"; x: number; y: number; base: Id[]; moved: boolean };

/** A marquee smaller than this (pixels) counts as a plain click. */
const MARQUEE_MIN_PX = 3;

/** The audio clip menu: opened by a right-click on the clip; on a fade it offers only that fade's shapes. */
export interface ClipMenu {
  clip: Id;
  at: MenuPoint;
  fade: FadeEdge | null;
}

/** A hint next to the cursor during a drag (the gain in dB). */
export interface DragTip {
  x: number;
  y: number;
  text: string;
}

/** Everything the pointer needs from the rest of the timeline (signals are passed as functions). */
export interface TimelinePointerDeps {
  /** Full-length timeline surface: mouse coordinates are measured from it (the canvas is only the visible window). */
  surface: () => HTMLElement;
  /** Timeline rows top to bottom: a track, or `null` for a folder row. */
  tracks: () => Lanes;
  clips: () => Clip[];
  selected: () => Id[];
  tool: () => Tool;
  snap: () => number;
  /** Tempo map: grid snapping uses its bars. */
  tempoMap: () => TempoMap;
  pxPerTick: () => number;
  /** Where every row is (`tracks/rowLayout.ts`). */
  layout: () => RowLayout;
  actions: ClipsActions;
  /** Clip selection; `track` is a track to select too (`null` means the track of the last selected clip). */
  select: (clips: Id[], track: Id | null) => void;
  setDraft: (draft: ClipDraft | null) => void;
  draft: () => ClipDraft | null;
  setMarquee: (rect: PixelRect | null) => void;
  /** Sample rate of an audio clip's sound, if known: the trim preview moves the offset by it. */
  sampleRate: (clip: Clip) => number | undefined;
  setTip: (tip: DragTip | null) => void;
  openMenu: (menu: ClipMenu) => void;
}

export function createTimelinePointer(deps: TimelinePointerDeps) {
  let drag: Drag | null = null;

  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.surface(), event);
    const row = rowAt(y, deps.layout());
    return { x, tick: tickAtX(x, deps.pxPerTick()), y, track: row === null ? undefined : (deps.tracks()[row] ?? undefined) };
  };
  const handleTicks = () => HANDLE_PX / deps.pxPerTick();
  const snapped = (tick: number) => snapNearestInBars(deps.tempoMap(), tick, deps.snap());
  /** Fade handle or gain line of an audio clip under the point (the clip's row is found from its track). */
  const audioZone = (clip: Clip, x: number, y: number): AudioZone | null => {
    const row = laneOf(deps.tracks(), clip.track);
    if (row < 0) return null;
    const rect = clipRect(clip, row, deps.pxPerTick(), deps.layout());
    return audioZoneAt(clip, rect, x, y, deps.pxPerTick(), showsGainLine(clip, deps.selected().includes(clip.id)));
  };

  /** Click on a clip with the Draw tool: updates the selection and starts a drag. */
  const pressClip = (hit: ClipHit, event: PointerEvent, tick: number, x: number, y: number) => {
    const additive = isAdditive(event);
    const id = hit.clip.id;
    const zone = additive ? null : audioZone(hit.clip, x, y);
    if (zone || ((hit.zone === "resize" || hit.zone === "trim") && !additive)) {
      deps.select([id], hit.clip.track);
      if (zone?.kind === "fade") drag = { mode: "fade", clip: hit.clip, edge: zone.edge };
      else if (zone?.kind === "gain") drag = { mode: "gain", clip: hit.clip, y };
      else drag = { mode: hit.zone === "trim" ? "trim" : "resize", clip: hit.clip };
      return;
    }
    // Clicking an already selected clip keeps the group, otherwise it couldn't be dragged.
    const ids = additive ? toggleId(deps.selected(), id) : deps.selected().includes(id) ? deps.selected() : [id];
    deps.select(ids, hit.clip.track);
    if (!ids.includes(id)) return;
    const group = deps.clips().filter((c) => ids.includes(c.id));
    drag = { mode: "move", lead: hit.clip, group, grab: tick - hit.clip.startTick };
  };

  const onDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    const { x, tick, y, track } = locate(event);
    capture(deps.surface(), event);
    const hit = track ? hitClip(deps.clips(), track.id, tick, handleTicks()) : null;
    if (!track || !hit) {
      // Empty space: marquee selection (with Shift it adds to the current selection).
      const additive = isAdditive(event);
      if (!additive) deps.select([], track?.id ?? null);
      drag = { mode: "marquee", x, y, base: additive ? deps.selected() : [], moved: false };
      return;
    }
    if (deps.tool() === "erase") {
      deps.select([hit.clip.id], track.id);
      void deps.actions.remove(hit.clip.id);
    } else if (deps.tool() === "split") {
      deps.select([hit.clip.id], track.id);
      void deps.actions.split(hit.clip.id, snapped(tick));
    } else pressClip(hit, event, tick, x, y);
  };

  const moveMarquee = (drag: Extract<Drag, { mode: "marquee" }>, x: number, y: number) => {
    const rect: PixelRect = { x: Math.min(drag.x, x), y: Math.min(drag.y, y), w: Math.abs(x - drag.x), h: Math.abs(y - drag.y) };
    if (!drag.moved && Math.max(rect.w, rect.h) < MARQUEE_MIN_PX) return;
    drag.moved = true;
    deps.setMarquee(rect);
    const found = clipsInArea(deps.clips(), deps.tracks(), {
      fromTick: rect.x / deps.pxPerTick(),
      toTick: (rect.x + rect.w) / deps.pxPerTick(),
      fromRow: clampRow(rect.y, deps.layout()),
      toRow: clampRow(rect.y + rect.h, deps.layout()),
    });
    deps.select(unionIds(drag.base, found), null);
  };

  /** No gesture: the cursor hints at what a press would do. */
  const hoverCursor = (tick: number, x: number, y: number, track: Track | undefined) => {
    const hit = track ? hitClip(deps.clips(), track.id, tick, handleTicks()) : null;
    const zone = hit ? audioZone(hit.clip, x, y) : null;
    const tool = deps.tool();
    deps.surface().style.cursor = tool === "split" ? "col-resize" : tool === "erase" ? "not-allowed"
      : zone?.kind === "gain" ? "ns-resize" : zone?.kind === "fade" ? "pointer"
      : hit?.zone === "resize" || hit?.zone === "trim" ? "ew-resize" : hit ? "grab" : "default";
  };

  /** Drafts of the one-clip edits: trimmed start, fade length, gain (with its dB next to the cursor). */
  const moveEdit = (drag: Extract<Drag, { mode: "trim" | "fade" | "gain" }>, event: PointerEvent, tick: number, x: number, y: number) => {
    const { clip } = drag;
    let edited: Clip;
    if (drag.mode === "trim") {
      const start = trimStartAt(clip, tick, deps.snap(), deps.tempoMap());
      const rate = deps.sampleRate(clip);
      edited = rate || clip.content.type === "notes"
        ? trimmedClip(clip, start, deps.tempoMap(), rate ?? 0)
        : { ...clip, startTick: start, lengthTicks: clip.startTick + clip.lengthTicks - start };
    } else if (drag.mode === "fade") {
      const length = fadeLengthAt(clip, drag.edge, snapped(tick));
      edited = drag.edge === "in" ? { ...clip, fadeInTicks: length } : { ...clip, fadeOutTicks: length };
    } else {
      edited = { ...clip, gain: draggedGain(clip.gain, y - drag.y, event.shiftKey) };
      deps.setTip({ x: x + 12, y: y - 12, text: t().tracks.clipEdit.gain(formatDb(edited.gain)) });
    }
    deps.setDraft({ placements: [], copy: false, edited });
  };

  const onMove = (event: PointerEvent) => {
    const { x, tick, y, track } = locate(event);
    if (!drag) return hoverCursor(tick, x, y, track);
    if (drag.mode === "trim" || drag.mode === "fade" || drag.mode === "gain") return moveEdit(drag, event, tick, x, y);
    if (drag.mode === "marquee") {
      moveMarquee(drag, x, y);
    } else if (drag.mode === "move") {
      const tracks = deps.tracks();
      // Snapping follows the lead clip (the one grabbed); the rest move by the same delta.
      const { lead } = drag;
      const deltaTick = snapped(tick - drag.grab) - lead.startTick;
      const copy = event.altKey;
      // A copy stays on its original tracks: its rows don't shift.
      const deltaRows = copy ? 0 : clampRow(y, deps.layout()) - laneOf(tracks, lead.track);
      deps.setDraft({ placements: moveGroup(drag.group, tracks, deltaTick, deltaRows), copy });
    } else {
      deps.setDraft({ placements: [{ id: drag.clip.id, ...resizePlacement(drag.clip, tick, deps.snap(), deps.tempoMap()) }], copy: false });
    }
  };

  /** Hands the finished gesture to the actions: they show the edit in the project at once and send it to the backend. */
  const commit = async (finished: Drag | null, draft: ClipDraft | null) => {
    const edited = draft?.edited;
    if (finished?.mode === "trim" && edited && edited.startTick !== finished.clip.startTick) {
      return deps.actions.trimStart(edited);
    }
    if (finished?.mode === "fade" && edited) {
      const [before, after] = finished.edge === "in" ? [finished.clip.fadeInTicks, edited.fadeInTicks] : [finished.clip.fadeOutTicks, edited.fadeOutTicks];
      if (before !== after) await deps.actions.edit(edited.id, finished.edge === "in" ? { fadeInTicks: after } : { fadeOutTicks: after });
      return;
    }
    if (finished?.mode === "gain" && edited && edited.gain !== finished.clip.gain) {
      await deps.actions.edit(edited.id, { gain: edited.gain });
      return;
    }
    if ((finished?.mode !== "move" && finished?.mode !== "resize") || !draft) return;
    if (finished.mode === "move" && draft.copy) {
      const shift = draft.placements[0]!.startTick - finished.group[0]!.startTick;
      if (shift !== 0) await deps.actions.copy(finished.group.map((c) => c.id), groupStart(finished.group) + shift);
    } else {
      await deps.actions.commitMany(draft.placements.map(({ id, ...edit }) => ({ id, edit })));
    }
  };

  const onUp = async () => {
    const finished = drag;
    const draft = deps.draft();
    drag = null;
    deps.setMarquee(null);
    deps.setTip(null);
    // The draft stays until the action is done: copies arrive only with the refreshed project, and without the
    // draft their ghosts would blink out for a frame.
    try {
      await commit(finished, draft);
    } finally {
      deps.setDraft(null);
    }
  };

  /** Double-click on empty space of an instrument track adds a one-bar clip (in the signature in effect there). */
  const onDouble = (event: MouseEvent) => {
    const { tick, track } = locate(event);
    if (!track?.instrument || deps.tool() !== "draw") return;
    const start = snapDownInBars(deps.tempoMap(), tick, deps.snap());
    const length = barLengthAt(deps.tempoMap(), start);
    if (hitClip(deps.clips(), track.id, tick, 0) || overlapsOthers(deps.clips(), track.id, start, length)) return;
    void deps.actions.addNote(track.id, start, length);
  };

  const onContext = (event: MouseEvent) => {
    event.preventDefault();
    const { tick, track } = locate(event);
    const hit = track ? hitClip(deps.clips(), track.id, tick, 0) : null;
    if (!hit) return;
    const { clip } = hit;
    if (clip.content.type !== "audio") return void deps.actions.remove(clip.id);
    if (!deps.selected().includes(clip.id)) deps.select([clip.id], clip.track);
    deps.openMenu({ clip: clip.id, at: menuPoint(event), fade: fadeAt(clip, tick) });
  };

  return { onDown, onMove, onUp, onDouble, onContext };
}
