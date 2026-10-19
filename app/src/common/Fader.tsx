// Vertical volume fader: mouse drag, wheel and arrow keys; double click resets to 0 dB. Outside a drag the cap glides
// to a new value (a reset, a step, an undo); while dragging it follows the pointer exactly.
import { createSignal } from "solid-js";
import { clamp } from "./math";
import { FADER_STEP, faderToVolume, nudgeFader, volumeToFader } from "../mixer/mixer";
import { capture, localPoint } from "./pointer";

const DEFAULT_HEIGHT = 150;
const UNITY_VOLUME = 1;

export function Fader(props: { volume: number; height?: number; onChange: (volume: number) => void; label: string }) {
  let track!: HTMLDivElement;
  const height = () => props.height ?? DEFAULT_HEIGHT;
  const position = () => volumeToFader(props.volume);
  const [dragging, setDragging] = createSignal(false);

  const setFromPointer = (event: PointerEvent) => {
    const raw = 1 - localPoint(track, event).y / track.getBoundingClientRect().height;
    props.onChange(faderToVolume(clamp(raw, 0, 1)));
  };

  return (
    <div
      ref={track}
      class="group relative w-7 cursor-ns-resize touch-none rounded-sm"
      style={{ height: `${height()}px` }}
      role="slider"
      aria-label={props.label}
      aria-valuemin={0}
      aria-valuemax={1.5}
      aria-valuenow={props.volume}
      tabIndex={0}
      onPointerDown={(e) => { capture(track, e); setDragging(true); setFromPointer(e); }}
      onPointerMove={(e) => e.buttons === 1 && setFromPointer(e)}
      onPointerUp={() => setDragging(false)}
      onPointerCancel={() => setDragging(false)}
      onDblClick={() => props.onChange(UNITY_VOLUME)}
      onWheel={(e) => { e.preventDefault(); props.onChange(nudgeFader(props.volume, e.deltaY < 0 ? FADER_STEP : -FADER_STEP)); }}
      onKeyDown={(e) => {
        if (e.key === "ArrowUp") props.onChange(nudgeFader(props.volume, FADER_STEP));
        if (e.key === "ArrowDown") props.onChange(nudgeFader(props.volume, -FADER_STEP));
      }}
    >
      <div class="absolute bottom-0 left-1/2 w-[3px] -translate-x-1/2 rounded-full bg-panel-3" style={{ height: "100%" }} />
      <div
        class="absolute bottom-0 left-1/2 w-[3px] -translate-x-1/2 rounded-full bg-accent"
        classList={{ "transition-[height] duration-150 ease-out": !dragging() }}
        style={{ height: `${position() * 100}%` }}
      />
      <div
        class="absolute left-0.5 h-3 w-6 -translate-y-1/2 rounded-sm border border-field-line bg-panel-2 duration-150 ease-out group-hover:border-accent"
        classList={{
          "transition-[bottom,border-color,box-shadow]": !dragging(),
          "transition-[border-color,box-shadow] border-accent shadow-[0_0_0_3px_var(--color-select)]": dragging(),
        }}
        style={{ bottom: `calc(${position() * 100}% - 6px)` }}
      >
        <div class="mx-auto mt-[4px] h-px w-3.5 bg-text" />
      </div>
    </div>
  );
}
