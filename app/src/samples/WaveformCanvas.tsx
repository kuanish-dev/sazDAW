// Sample waveform: wheel zoom, range selection by dragging, slice markers on the ruler.
// The component only wires things up: `waveformRender` draws, `waveformPointer` handles the mouse; data and state belong to the parent.
import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import type { Waveform } from "../api/types";
import type { Selection, View } from "./waveform";
import { createWaveformPointer } from "./waveformPointer";
import { drawWaveform } from "./waveformRender";

const MIN_WIDTH = 200;
const START_WIDTH = 800;

export function WaveformCanvas(props: {
  wave: Waveform | null;
  total: number;
  sampleRate: number;
  view: View;
  selection: Selection | null;
  markers: number[];
  playhead: number | null;
  onView: (view: View) => void;
  onSelection: (selection: Selection | null) => void;
  onCursor: (frame: number) => void;
  onMarkers: (markers: number[]) => void;
  onWidth: (width: number) => void;
}) {
  let canvas!: HTMLCanvasElement;
  let host!: HTMLDivElement;
  const [width, setWidth] = createSignal(START_WIDTH);
  const [hover, setHover] = createSignal<number | null>(null);
  /** Markers including the one being dragged. */
  const [draftMarkers, setDraftMarkers] = createSignal<number[] | null>(null);

  onMount(() => {
    const observer = new ResizeObserver(() => {
      const w = Math.max(MIN_WIDTH, Math.floor(host.clientWidth));
      setWidth(w);
      props.onWidth(w);
    });
    observer.observe(host);
    onCleanup(() => observer.disconnect());
  });

  createEffect(() =>
    drawWaveform(canvas, {
      width: width(), wave: props.wave, sampleRate: props.sampleRate, view: props.view, selection: props.selection,
      markers: draftMarkers() ?? props.markers, hover: hover(), playhead: props.playhead,
    }),
  );

  const pointer = createWaveformPointer({
    canvas: () => canvas, width, total: () => props.total, view: () => props.view, selection: () => props.selection,
    markers: () => props.markers, setHover, draftMarkers, setDraftMarkers,
    onView: (view) => props.onView(view), onSelection: (selection) => props.onSelection(selection),
    onCursor: (frame) => props.onCursor(frame), onMarkers: (markers) => props.onMarkers(markers),
  });

  return (
    <div ref={host} class="w-full overflow-hidden rounded border border-line">
      <canvas
        ref={canvas}
        class="block touch-none"
        onPointerDown={pointer.onDown}
        onPointerMove={pointer.onMove}
        onPointerUp={pointer.onUp}
        onPointerLeave={pointer.onLeave}
        onDblClick={pointer.onDouble}
        onContextMenu={pointer.onContext}
        onWheel={pointer.onWheel}
      />
    </div>
  );
}
