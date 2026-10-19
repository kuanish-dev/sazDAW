// Mouse on the ruler strip (what the signature and marker flags leave to it): a click sets the cue (the start position play
// and stop return to) and moves the playhead there, a drag draws the loop; right-click on the cue flag removes the cue,
// anywhere else it clears the loop. Each handler returns `true` if it owns the event, like `tempoPointer`/`markerPointer`.
import type { TempoMap } from "../api/types";
import { snapNearestInBars } from "../common/tempoMap";
import { capture, localPoint } from "../common/pointer";
import { onCueFlag } from "./cueLayout";
import { tickAtX } from "./timelineLayout";

export function createRulerPointer(deps: {
  surface: () => HTMLElement;
  pxPerTick: () => number;
  snap: () => number;
  tempoMap: () => TempoMap;
  cue: () => number | null;
  cueAt: (tick: number) => void;
  clearCue: () => void;
  setLoop: (region: [number, number] | null) => void;
  loopDraft: () => [number, number] | null;
  setLoopDraft: (loop: [number, number] | null) => void;
}) {
  /** Where the press snapped to: the cue of a click and the anchor of a loop drag. */
  let anchor: number | null = null;

  const locate = (event: MouseEvent) => {
    const { x, y } = localPoint(deps.surface(), event);
    return { x, y, tick: snapNearestInBars(deps.tempoMap(), tickAtX(x, deps.pxPerTick()), deps.snap()) };
  };

  return {
    onDown(event: PointerEvent): boolean {
      if (event.button !== 0) return false;
      capture(deps.surface(), event);
      anchor = locate(event).tick;
      return true;
    },
    onMove(event: PointerEvent): boolean {
      if (anchor === null) return false;
      const end = locate(event).tick;
      if (end !== anchor) deps.setLoopDraft([Math.min(anchor, end), Math.max(anchor, end)]);
      return true;
    },
    onUp(): boolean {
      if (anchor === null) return false;
      const loop = deps.loopDraft();
      const at = anchor;
      anchor = null;
      deps.setLoopDraft(null);
      if (loop) deps.setLoop(loop);
      else deps.cueAt(Math.round(at));
      return true;
    },
    /** Right-click on the cue flag: checked before marker flags, the cue sits on top of them. */
    onCueContext(event: MouseEvent): boolean {
      const { x, y } = localPoint(deps.surface(), event);
      if (!onCueFlag(deps.cue(), x, y, deps.pxPerTick())) return false;
      event.preventDefault();
      deps.clearCue();
      return true;
    },
    onContext(event: MouseEvent): boolean {
      event.preventDefault();
      deps.setLoop(null);
      return true;
    },
  };
}
