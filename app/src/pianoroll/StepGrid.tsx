// Step view of a clip: a row for each pitch that has notes (plus the track's default pitch); for drums this is
// a single beat row. A click toggles a one-step note, dragging paints along the same row.
import { For, createMemo, onCleanup, onMount } from "solid-js";
import type { Track } from "../api/types";
import { clipBarTicks, clipNotes, ghostTicks, playheadInLoop, type NoteClip } from "./noteClip";
import { pitchName } from "../common/pitch";
import { activeStepOf, stepGroupOdd, stepOn, stepRows, stepTotal } from "./steps";
import { t } from "../i18n";
import { useSession } from "../session";

/** How many steps fit without scrolling (a bar); longer — cells get wider and scrolling appears. */
const STEPS_WITHOUT_SCROLL = 16;
const MIN_CELL_PX = 14;
const MIN_CELL_PX_SCROLLING = 20;
/** When scrolling: space per cell (with the gap) and for the pitch label on the left. */
const CELL_PITCH_PX = 23;
const ROW_LABEL_PX = 90;

export function StepGrid(props: { clip: NoteClip; track: Track }) {
  const session = useSession();
  const { notes: notesActions } = session;

  const notes = () => clipNotes(props.clip);
  const rows = createMemo(() => stepRows(notes(), props.track.defaultPitch));
  const realSteps = () => stepTotal(props.clip.content.loopTicks);
  /** The spare bar: inactive while it has no notes; placing a note extends the clip. */
  const totalSteps = () => stepTotal(props.clip.content.loopTicks, ghostTicks(props.clip.content, clipBarTicks(session.project.tempoMap, props.clip)));
  const scrolling = () => totalSteps() > STEPS_WITHOUT_SCROLL;
  const activeStep = () => {
    const position = session.playhead();
    return position === null ? null : activeStepOf(playheadInLoop(props.clip.startTick, props.clip.lengthTicks, props.clip.content.loopTicks, position), realSteps());
  };

  // Drag painting: the mode is set by the first press and applies to the same row.
  let paint: { mode: "add" | "remove"; pitch: number } | null = null;
  onMount(() => {
    const stop = () => { paint = null; };
    window.addEventListener("pointerup", stop);
    onCleanup(() => window.removeEventListener("pointerup", stop));
  });

  const apply = (pitch: number, step: number, mode: "add" | "remove") => void notesActions.setStep(props.clip.id, step, mode, pitch);

  const columns = () => `repeat(${totalSteps()}, minmax(${scrolling() ? MIN_CELL_PX_SCROLLING : MIN_CELL_PX}px, 1fr))`;

  return (
    <div class="min-h-0 flex-1 overflow-auto p-3" aria-label={t().pianoRoll.steps}>
      <div class="flex min-w-max flex-col gap-1" style={{ "min-width": scrolling() ? `${totalSteps() * CELL_PITCH_PX + ROW_LABEL_PX}px` : "100%" }}>
        <For each={rows()}>
          {(pitch) => (
            <div class="flex items-center gap-2">
              <span class="sticky left-0 z-10 w-12 shrink-0 bg-panel text-right text-xs text-muted" title={t().pianoRoll.pitchTitle(pitchName(pitch))}>{pitchName(pitch)}</span>
              <div class="grid min-w-0 flex-1 gap-[3px]" style={{ "grid-template-columns": columns() }}>
                <For each={Array.from({ length: totalSteps() }, (_, i) => i)}>
                  {(step) => {
                    const on = () => stepOn(notes(), pitch, step);
                    const ghost = () => step >= realSteps();
                    return (
                      <button
                        class="h-7 rounded-[3px] transition-colors"
                        classList={{
                          "bg-accent hover:brightness-110": on(),
                          "bg-panel-3 hover:brightness-125": !on() && !ghost() && !stepGroupOdd(step),
                          "bg-[var(--step-alt)] hover:brightness-125": !on() && !ghost() && stepGroupOdd(step),
                          "border border-dashed border-accent/30 bg-transparent opacity-50 hover:border-accent hover:bg-accent/20 hover:opacity-100": !on() && ghost(),
                          "outline-solid outline-2 -outline-offset-2 outline-text": activeStep() === step,
                        }}
                        aria-label={t().pianoRoll.stepAria(pitchName(pitch), step + 1)}
                        aria-pressed={on()}
                        title={ghost() ? t().pianoRoll.ghostStepHint : undefined}
                        onPointerDown={(e) => {
                          e.preventDefault();
                          paint = { mode: on() ? "remove" : "add", pitch };
                          apply(pitch, step, paint.mode);
                        }}
                        onPointerEnter={() => paint && paint.pitch === pitch && apply(pitch, step, paint.mode)}
                      />
                    );
                  }}
                </For>
              </div>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}
