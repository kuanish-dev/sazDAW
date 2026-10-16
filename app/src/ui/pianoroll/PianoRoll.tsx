// Piano roll of the selected note clip. Assembly only: view state (grid, zoom, selection, drafts, scale); drawing —
// `render`, mouse — `pointer` and `velocityPointer`, editing and keys — `editing`, musical edits — `NoteTools` (in the
// toolbar), controller lanes — `ControllerLaneView`. Note times are relative to the clip start.
// The panel handles keys itself while focused (`data-owns-keys`), so timeline clip keys do not affect it.
import { createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { STEP_TICKS, type NewNote, type Note } from "../../api/types";
import { clipBarTicks, clipNotes, playheadInLoop, viewTicks, type NoteClip } from "../../domain/noteClip";
import type { ScaleChoice } from "../../domain/noteTransforms";
import { changedNotes, type TickPitchRect } from "../../domain/selection";
import { signatureAt } from "../../domain/tempoMap";
import { BAR_GRID, FREE_GRID, GRID_CHOICES } from "../../domain/time";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { isZoomWheel } from "../common/pointer";
import { createAudition } from "./audition";
import { ControllerLaneView } from "./ControllerLaneView";
import { createNoteEditing } from "./editing";
import { BASE_PX_PER_TICK, FREE_MIN_TICKS, KEYS_WIDTH, nextZoom, rowY, tickX } from "./pianoRollLayout";
import { PianoRollToolbar } from "./PianoRollToolbar";
import { createPianoRollPointer } from "./pointer";
import { drawGrid, drawVelocity } from "./render";
import { createVelocityPointer } from "./velocityPointer";

export function PianoRoll(props: { clip: NoteClip; defaultPitch: number }) {
  const session = useSession();
  const { notes: notesActions, live } = session.actions;

  const [snap, setSnap] = createSignal(STEP_TICKS);
  const [zoom, setZoom] = createSignal(1);
  const [drafts, setDrafts] = createSignal<Record<number, NewNote>>({});
  const [selected, setSelected] = createSignal<Set<number>>(new Set());
  const [marquee, setMarquee] = createSignal<TickPitchRect | null>(null);
  // Width of the vertical scrollbar: the velocity lane is narrowed by it so the columns line up.
  const [gutter, setGutter] = createSignal(0);
  const [scrollLeft, setScrollLeft] = createSignal(0);
  /** Scale for "Fit to scale" and row highlighting; highlighting is off until turned on. */
  const [scale, setScale] = createSignal<ScaleChoice>({ root: 0, scale: "major" });
  const [highlightScale, setHighlightScale] = createSignal(false);
  let section!: HTMLElement;
  let gridCanvas!: HTMLCanvasElement;
  let velocityCanvas!: HTMLCanvasElement;
  let scroller!: HTMLDivElement;
  let velocityHost!: HTMLDivElement;

  const target = () => props.clip.id;
  const pxPerTick = () => BASE_PX_PER_TICK * zoom();
  /** Inside the clip the grid is even, by the time signature at its start: that signature's bar and beat. */
  const barTicks = () => clipBarTicks(session.project.tempoMap, props.clip);
  const beatTicks = () => barTicks() / signatureAt(session.project.tempoMap, props.clip.startTick).signature.numerator;
  /** Grid in ticks: "Bar" means the clip's bar. */
  const grid = () => (snap() === BAR_GRID ? barTicks() : snap());
  /** Visible length: the clip content plus a translucent spare bar where a note can be placed. */
  const viewLength = () => viewTicks(props.clip, barTicks());
  const width = () => KEYS_WIDTH + viewLength() * pxPerTick();
  /** Snap step; `free` (Alt held) or the no-snap mode disables snapping. */
  const stepFor = (free = false) => (free || snap() === FREE_GRID ? FREE_GRID : grid());
  /** Minimum note length: without snapping, notes can be very short. */
  const minLength = (free = false) => (stepFor(free) === FREE_GRID ? FREE_MIN_TICKS : grid());
  /** Step for grid operations (quantize, duplicate, paste): 1/16 without snapping. */
  const quantizeStep = () => (snap() === FREE_GRID ? STEP_TICKS : grid());

  /** The clip's notes, with dragged ones in their draft positions. */
  const noteList = createMemo(() => {
    const d = drafts();
    return clipNotes(props.clip).map((n) => (d[n.id] ? { id: n.id, ...d[n.id]! } : n));
  });
  const selectedNotes = createMemo(() => noteList().filter((n) => selected().has(n.id)));

  // The selection does not survive a clip change or notes that disappeared.
  createEffect(() => {
    const ids = new Set(clipNotes(props.clip).map((n) => n.id));
    setSelected((current) => {
      const kept = [...current].filter((id) => ids.has(id));
      return kept.length === current.size ? current : new Set(kept);
    });
  });

  const audition = createAudition(() => props.clip.track, live);
  onCleanup(audition.stop);

  /** Where to paste the clipboard: the last pointer position over the grid. */
  let pasteTick = 0;

  const editing = createNoteEditing({
    notes: notesActions, target, noteList, selectedNotes, selected, setSelected, quantizeStep,
    gridStep: () => minLength(),
    barTicks,
    pasteAt: () => pasteTick,
  });

  /** End of a mouse gesture: changed drafts as one edit; the action shows them at once, so dropping the drafts after it doesn't make notes jump. */
  const commitDrafts = async (group: Note[]) => {
    const changes = changedNotes(drafts(), group);
    await editing.applyChanges(changes);
    setDrafts({});
    return changes;
  };

  const pointer = createPianoRollPointer({
    notes: notesActions, audition, target, noteList, selected, selectedNotes, setSelected, setDrafts, marquee, setMarquee,
    gridCanvas: () => gridCanvas, pxPerTick, viewLength, stepFor, minLength, quantizeStep, commitDrafts,
    removeSelected: editing.removeSelected,
    setPasteTick: (tick) => { pasteTick = tick; },
  });
  const velocity = createVelocityPointer({
    velocityCanvas: () => velocityCanvas, noteList, selected, selectedNotes, pxPerTick, setDrafts, commitDrafts,
  });

  const playheadX = () => {
    const position = session.playhead();
    if (position === null) return null;
    const local = playheadInLoop(props.clip.startTick, props.clip.lengthTicks, props.clip.content.loopTicks, position);
    return local === null ? null : tickX(local, pxPerTick());
  };

  createEffect(() => {
    drawGrid(gridCanvas, {
      width: width(), pxPerTick: pxPerTick(), viewLength: viewLength(), contentTicks: props.clip.content.loopTicks, lineStep: quantizeStep(),
      barTicks: barTicks(), beatTicks: beatTicks(),
      notes: noteList(), selected: selected(), pressedPitches: session.keyboard.pressedPitches(), marquee: marquee(), playheadX: playheadX(),
      scale: highlightScale() ? scale() : null,
    });
  });
  createEffect(() => drawVelocity(velocityCanvas, { width: width(), pxPerTick: pxPerTick(), notes: noteList(), selected: selected() }));

  onMount(() => {
    const observer = new ResizeObserver(() => setGutter(scroller.offsetWidth - scroller.clientWidth));
    observer.observe(scroller);
    onCleanup(() => observer.disconnect());
  });

  /** Scrolls to the working range: around the clip's notes or the track's default pitch. */
  const centerOnNotes = () => {
    if (!scroller) return;
    const list = noteList();
    const center = list.length ? list.reduce((sum, n) => sum + n.pitch, 0) / list.length : props.defaultPitch;
    scroller.scrollTop = Math.max(0, rowY(center) - scroller.clientHeight / 2);
  };
  // Layout is not ready at mount time, so wait a frame; again on clip change.
  let lastCentered: number | undefined;
  createEffect(() => {
    const id = props.clip.id;
    if (id === lastCentered) return;
    lastCentered = id;
    requestAnimationFrame(centerOnNotes);
  });

  /** A press in the panel gives it the keyboard (Delete, ⌘A/C/V/D, arrows) without scrolling the page; inputs take focus themselves. */
  const takeKeyboard = (e: PointerEvent) => {
    if (!(e.target as HTMLElement).closest("input, select, textarea, button")) section.focus({ preventScroll: true });
  };

  const onWheel = (e: WheelEvent) => {
    if (!isZoomWheel(e)) return;
    e.preventDefault();
    setZoom(nextZoom(zoom(), e.deltaY < 0));
  };

  return (
    <section
      ref={section} class="flex h-full min-h-0 flex-col outline-none" aria-label={t().pianoRoll.title}
      tabIndex={0} data-owns-keys onKeyDown={editing.onKeyDown} onPointerDown={takeKeyboard}
    >
      <PianoRollToolbar
        snap={snap()} zoom={zoom()} onSnap={setSnap} onZoom={setZoom}
        onImportMidi={() => void notesActions.importInto(target())}
        onExportMidi={() => void notesActions.exportClip(target())}
        selectedCount={selected().size} hasNotes={noteList().length > 0} canPaste={notesActions.canPaste()}
        onCopy={editing.copySelected} onPaste={() => void editing.paste()} onDuplicate={() => void editing.duplicate()}
        onRemove={() => void editing.removeSelected()}
        onClear={() => void notesActions.clear(target())}
        tools={{
          clip: target(), ids: selected().size ? [...selected()] : undefined, hasNotes: noteList().length > 0,
          grid: quantizeStep(), gridLabel: GRID_CHOICES.find((c) => c.ticks === snap())?.label ?? "",
          scale: scale(), onScale: setScale, highlight: highlightScale(), onHighlight: setHighlightScale,
        }}
      />

      {/* The grid scrolls both ways; the velocity lane follows the horizontal scroll. */}
      <div
        ref={scroller}
        class="min-h-0 flex-1 overflow-auto"
        tabIndex={-1}
        onScroll={() => {
          if (velocityHost) velocityHost.scrollLeft = scroller.scrollLeft;
          setScrollLeft(scroller.scrollLeft);
        }}
        onWheel={onWheel}
      >
        <canvas
          ref={gridCanvas} class="block touch-none"
          onPointerDown={pointer.onDown} onPointerMove={pointer.onMove} onPointerUp={() => void pointer.finish()} onContextMenu={pointer.onContextMenu}
        />
      </div>
      <div ref={velocityHost} class="shrink-0 overflow-hidden border-t border-line" style={{ "margin-right": `${gutter()}px` }}>
        <canvas ref={velocityCanvas} class="block touch-none" onPointerDown={velocity.onDown} onPointerMove={velocity.onMove} onPointerUp={() => void velocity.onUp()} />
      </div>
      <ControllerLaneView
        clip={props.clip} width={width()} pxPerTick={pxPerTick()} viewLength={viewLength()} lineStep={quantizeStep()}
        barTicks={barTicks()} beatTicks={beatTicks()} step={snap() === FREE_GRID ? FREE_MIN_TICKS : grid()}
        scrollLeft={scrollLeft()} gutter={gutter()}
      />
      <p class="border-t border-line px-3 py-1.5 text-[11px] leading-relaxed text-muted">
        {t().pianoRoll.help}
      </p>
    </section>
  );
}
