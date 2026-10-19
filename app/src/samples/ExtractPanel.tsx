// Panel for extracting sound from a file: selection, name and root note of a new sampler track, slicing at markers, saving to WAV.
import { For } from "solid-js";
import { pitchName } from "../common/pitch";
import { formatTime, type Selection } from "./waveform";
import { t } from "../i18n";

/** Root note choices: C2 … C7. */
const ROOT_NOTE_LOW = 36;
const ROOT_NOTE_COUNT = 61;
const ROOT_NOTES = Array.from({ length: ROOT_NOTE_COUNT }, (_, i) => ROOT_NOTE_LOW + i);

export function ExtractPanel(props: {
  selection: Selection | null;
  rate: number;
  name: string;
  onName: (name: string) => void;
  root: number;
  onRoot: (root: number) => void;
  markerCount: number;
  onExtractSelection: () => void;
  onExtractSlices: () => void;
  onSaveRegion: () => void;
}) {
  const selectionText = () => {
    const s = props.selection;
    if (!s) return t().samples.noSelection;
    return t().samples.selectionRange(formatTime(s.from, props.rate), formatTime(s.to, props.rate), ((s.to - s.from) / props.rate).toFixed(3));
  };
  const msg = () => t().samples;

  return (
    <div class="mt-auto grid gap-4 border-t border-line bg-panel p-3 md:grid-cols-[1fr_auto]">
      <div class="flex flex-col gap-3">
        <div class="flex flex-wrap items-center gap-3">
          <span class="label">{msg().selection}</span>
          <span class="font-mono text-xs">{selectionText()}</span>
        </div>
        <div class="flex flex-wrap items-end gap-3">
          <label class="flex flex-col gap-1">
            <span class="label">{msg().trackName}</span>
            <input class="field w-56" value={props.name} onInput={(e) => props.onName(e.currentTarget.value)} spellcheck={false} aria-label={msg().trackName} />
          </label>
          <label class="flex flex-col gap-1">
            <span class="label" title={msg().rootNoteHint}>{msg().rootNote}</span>
            <select class="field w-24" value={props.root} onChange={(e) => props.onRoot(Number(e.currentTarget.value))} aria-label={msg().rootNote}>
              <For each={ROOT_NOTES}>{(n) => <option value={n}>{pitchName(n)}</option>}</For>
            </select>
          </label>
        </div>
      </div>
      <div class="flex flex-wrap items-end justify-end gap-2">
        <button class="btn btn-primary" disabled={!props.selection} title={msg().toSamplerHint} onClick={() => props.onExtractSelection()}>
          {msg().toSampler}
        </button>
        <button class="btn" disabled={props.markerCount === 0} title={msg().sliceHint} onClick={() => props.onExtractSlices()}>
          {msg().slice(props.markerCount > 0 ? props.markerCount + 1 : 0)}
        </button>
        <button class="btn" title={msg().saveWavHint} onClick={() => props.onSaveRegion()}>{msg().saveWav}</button>
      </div>
    </div>
  );
}
