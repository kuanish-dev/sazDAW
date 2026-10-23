import { formatTime } from "./waveform";
import { t } from "../i18n";
import { IconFit, IconFlag, IconLoop, IconPlay, IconStop, IconZoomIn, IconZoomOut } from "../common/icons";
import { ToolButton, Toolbar, ToolbarDivider } from "../common/Toolbar";

export function SampleToolbar(props: {
  playing: boolean;
  onTogglePlay: () => void;
  loop: boolean;
  onLoop: (loop: boolean) => void;
  onZoom: (factor: number) => void;
  onFitAll: () => void;
  /** "Fit selection": `null` — no selection, the button is disabled. */
  onFitSelection: (() => void) | null;
  /** "Marker": `null` — no place chosen (no cursor or selection), the button is disabled. */
  onAddMarker: (() => void) | null;
  markerCount: number;
  onClearMarkers: () => void;
  total: number;
  rate: number;
}) {
  const msg = () => t().samples;
  return (
    <Toolbar>
      <button class="btn w-24 text-xs" classList={{ "btn-on": props.playing }} title={msg().playHint} onClick={() => props.onTogglePlay()}>
        {props.playing ? <IconStop size={14} /> : <IconPlay size={14} />}{props.playing ? msg().stop : msg().play}
      </button>
      <ToolButton icon={<IconLoop />} label={msg().loop} title={msg().loopHint} pressed={props.loop} onClick={() => props.onLoop(!props.loop)} />
      <ToolbarDivider />
      <ToolButton icon={<IconZoomIn />} label={msg().zoomIn} onClick={() => props.onZoom(2)} />
      <ToolButton icon={<IconZoomOut />} label={msg().zoomOut} onClick={() => props.onZoom(0.5)} />
      <ToolButton icon={<IconFit />} label={msg().all} title={msg().allHint} text onClick={() => props.onFitAll()} />
      <button class="btn btn-ghost !px-2 text-xs" disabled={!props.onFitSelection} title={msg().fitSelectionHint} onClick={() => props.onFitSelection?.()}>{msg().fitSelection}</button>
      <ToolbarDivider />
      <ToolButton icon={<IconFlag />} label={msg().marker} title={msg().markerHint} text disabled={!props.onAddMarker} onClick={() => props.onAddMarker?.()} />
      <button class="btn btn-ghost !px-2 text-xs" disabled={props.markerCount === 0} title={msg().clearMarkersHint} onClick={() => props.onClearMarkers()}>{msg().clearMarkers}</button>
      <span class="ml-auto pr-1 font-mono text-[11px] text-muted">{formatTime(props.total, props.rate)} · {msg().hertz(props.rate)}</span>
    </Toolbar>
  );
}
