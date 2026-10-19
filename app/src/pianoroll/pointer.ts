// Mouse on the piano roll grid: creating, moving and resizing notes, marquee selection, key audition, right click.
// Holds the gesture and note "drafts" while dragging; the result goes to `Notes` (via `commitDrafts`).
import type { Notes } from "./notes";
import { STEP_TICKS, type Id, type NewNote, type Note } from "../api/types";
import { fitNote, hitTest, stripId } from "./noteFit";
import { DEFAULT_VELOCITY } from "../common/pitch";
import { moveNotes, notesInRect, toggleInSet, type NoteChange, type TickPitchRect } from "./noteSelection";
import { snapDown, snapNearest } from "../common/time";
import { capture, localPoint } from "../common/pointer";
import type { Audition } from "./audition";
import { HANDLE_PX, KEYS_WIDTH, pitchAtY, tickAtX } from "./pianoRollLayout";

type Drag =
  | { mode: "move"; anchor: Note; grab: number; group: Note[] }
  | { mode: "resize"; anchor: Note; group: Note[] }
  /** New note: `created` is the backend reply; `id` is known once it arrives (until then there is no draft). */
  | { mode: "create"; created: Promise<Note | undefined>; id: Id | null; origin: NewNote }
  | { mode: "marquee"; startTick: number; startPitch: number; additive: boolean };

export interface PianoRollPointerDeps {
  notes: Notes;
  audition: Audition;
  gridCanvas(): HTMLCanvasElement;
  /** Clip being edited; `null` if no clip is selected. */
  target(): Id | null;
  noteList(): Note[];
  selected(): Set<number>;
  selectedNotes(): Note[];
  setSelected(ids: Set<number>): void;
  setDrafts(drafts: Record<number, NewNote>): void;
  marquee(): TickPitchRect | null;
  setMarquee(box: TickPitchRect | null): void;
  pxPerTick(): number;
  viewLength(): number;
  /** Snap step (`free`: Alt held, snapping off) and minimum note length. */
  stepFor(free: boolean): number;
  minLength(free: boolean): number;
  quantizeStep(): number;
  /** Sends the group's changed drafts as one edit and clears the drafts; returns what was sent. */
  commitDrafts(group: Note[]): Promise<NoteChange[]>;
  removeSelected(): Promise<void>;
  /** Reports the grid position under the pointer: clipboard pastes go here. */
  setPasteTick(tick: number): void;
}

export function createPianoRollPointer(deps: PianoRollPointerDeps) {
  let drag: Drag | null = null;
  /** Length of the last created or resized note: new notes are drawn with the same length. */
  let lastLength = STEP_TICKS;

  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.gridCanvas(), event);
    return { tick: tickAtX(x, deps.pxPerTick()), pitch: pitchAtY(y), inKeys: x < KEYS_WIDTH };
  };
  const hitAt = (tick: number, pitch: number, handlePx = HANDLE_PX) => hitTest(deps.noteList(), tick, pitch, handlePx / deps.pxPerTick());
  const setDraftsFor = (list: NoteChange[]) => deps.setDrafts(Object.fromEntries(list.map(({ id, note }) => [id, note])));

  /** Press on a note: Shift toggles it in the selection, otherwise moves or resizes the selected group. */
  const pressNote = (hit: NonNullable<ReturnType<typeof hitAt>>, event: PointerEvent, tick: number) => {
    if (event.shiftKey) return deps.setSelected(toggleInSet(deps.selected(), hit.note.id));
    if (!deps.selected().has(hit.note.id)) deps.setSelected(new Set([hit.note.id]));
    const group = deps.selectedNotes();
    deps.audition.start(hit.note.pitch);
    drag = hit.zone === "resize"
      ? { mode: "resize", anchor: hit.note, group }
      : { mode: "move", anchor: hit.note, grab: tick - hit.note.startTick, group };
  };

  /** Press on empty space: the note is created on the backend right away, and its length is dragged while the backend replies. */
  const startCreate = (clip: Id, event: PointerEvent, tick: number, pitch: number) => {
    deps.setSelected(new Set<number>());
    const free = event.altKey;
    const origin = fitNote(
      { pitch, startTick: snapDown(tick, deps.stepFor(free)), lengthTicks: Math.max(lastLength, deps.minLength(free)), velocity: DEFAULT_VELOCITY },
      deps.viewLength(),
      deps.minLength(free),
    );
    deps.audition.start(pitch);
    const gesture: Drag = { mode: "create", created: deps.notes.addNote(clip, origin), id: null, origin };
    drag = gesture;
    void gesture.created.then((note) => {
      if (note) gesture.id = note.id;
    });
  };

  const onDown = (event: PointerEvent) => {
    const clip = deps.target();
    if (event.button !== 0 || clip === null) return;
    const { tick, pitch, inKeys } = locate(event);
    capture(deps.gridCanvas(), event);
    drag = null;
    if (inKeys) return deps.audition.start(pitch);
    const hit = hitAt(tick, pitch);
    if (hit) return pressNote(hit, event, tick);
    if (event.shiftKey) {
      drag = { mode: "marquee", startTick: tick, startPitch: pitch, additive: true };
      return deps.setMarquee({ fromTick: tick, toTick: tick, fromPitch: pitch, toPitch: pitch });
    }
    startCreate(clip, event, tick, pitch);
  };

  const onMove = (event: PointerEvent) => {
    const { tick, pitch } = locate(event);
    deps.setPasteTick(snapDown(tick, deps.quantizeStep()));
    if (!drag) {
      const hit = hitAt(tick, pitch);
      deps.gridCanvas().style.cursor = hit?.zone === "resize" ? "ew-resize" : hit ? "grab" : "crosshair";
      return;
    }
    const step = deps.stepFor(event.altKey);
    const least = deps.minLength(event.altKey);
    if (drag.mode === "marquee") {
      deps.setMarquee({ fromTick: drag.startTick, toTick: tick, fromPitch: drag.startPitch, toPitch: pitch });
    } else if (drag.mode === "move") {
      const moved = moveNotes(drag.group, snapNearest(tick - drag.grab, step) - drag.anchor.startTick, pitch - drag.anchor.pitch);
      setDraftsFor(moved);
      const anchorId = drag.anchor.id;
      const anchorNow = moved.find((m) => m.id === anchorId);
      if (anchorNow) deps.audition.start(anchorNow.note.pitch);
    } else if (drag.mode === "resize") {
      const deltaEnd = snapNearest(tick, step) - (drag.anchor.startTick + drag.anchor.lengthTicks);
      setDraftsFor(drag.group.map((n) => ({ id: n.id, note: { ...stripId(n), lengthTicks: Math.max(least, n.lengthTicks + deltaEnd) } })));
    } else if (drag.mode === "create" && drag.id !== null) {
      const length = Math.max(least, snapNearest(tick, step) - drag.origin.startTick);
      setDraftsFor([{ id: drag.id, note: fitNote({ ...drag.origin, lengthTicks: length }, deps.viewLength(), least) }]);
    }
  };

  const finishMarquee = (additive: boolean) => {
    const box = deps.marquee();
    deps.setMarquee(null);
    if (!box) return;
    const hit = notesInRect(deps.noteList(), box).map((n) => n.id);
    deps.setSelected(new Set<number>([...(additive ? deps.selected() : []), ...hit]));
  };

  /** Waits for the backend reply: a note released before the reply is not lost, and the draft's length still gets applied. */
  const finishCreate = async (gesture: Extract<Drag, { mode: "create" }>) => {
    const note = await gesture.created;
    if (!note) return deps.setDrafts({});
    const changes = await deps.commitDrafts([note]);
    lastLength = changes[0]?.note.lengthTicks ?? note.lengthTicks;
    deps.setSelected(new Set([note.id]));
  };

  const finishEdit = async (gesture: Extract<Drag, { mode: "move" | "resize" }>) => {
    const changes = await deps.commitDrafts(gesture.group);
    const anchorId = gesture.anchor.id;
    if (gesture.mode === "resize") lastLength = changes.find((c) => c.id === anchorId)?.note.lengthTicks ?? lastLength;
  };

  const finish = async () => {
    const finished = drag;
    drag = null;
    deps.audition.stop();
    if (!finished) return deps.setDrafts({});
    if (finished.mode === "marquee") return finishMarquee(finished.additive);
    if (finished.mode === "create") return finishCreate(finished);
    return finishEdit(finished);
  };

  /** Right click: on a selected note removes the whole selection, on any other note only that note. */
  const onContextMenu = (event: MouseEvent) => {
    event.preventDefault();
    const clip = deps.target();
    if (clip === null) return;
    const { tick, pitch } = locate(event);
    const hit = hitAt(tick, pitch, 0);
    if (!hit) return;
    if (deps.selected().has(hit.note.id) && deps.selected().size > 1) void deps.removeSelected();
    else void deps.notes.removeNote(clip, hit.note.id);
  };

  return { onDown, onMove, finish, onContextMenu };
}
