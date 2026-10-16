// "Export audio" dialog: destination, format (WAV/FLAC/MP3) and its quality, sample rate, loudness matching, song range,
// tail, dither and stems. Which settings apply to which format and the request built from the form are
// `domain/exportAudio.ts`; rendering and the result message (with loudness and peak) are the `audioExport.render` action.
import { For, Show, createSignal } from "solid-js";
import type { BitDepth, ExportFormat, Mp3Bitrate } from "../../api/types";
import {
  DEFAULT_CEILING_DB, DEFAULT_SAMPLE_RATE, DEFAULT_TAIL_SECONDS, EXPORT_FORMATS, LOUDNESS_CHOICES, MAX_TAIL_SECONDS, MP3_BITRATES,
  SAMPLE_RATES, bitDepthsFor, clipsRange, defaultDither, exportOptions, fitBitDepth, isIntegerPcm, pathForFormat,
  type LoudnessChoice, type RangeChoice,
} from "../../domain/exportAudio";
import { t } from "../../i18n";
import { useSession } from "../../state/session";
import { Modal } from "../common/Modal";

export function ExportDialog(props: { onClose: () => void }) {
  const session = useSession();
  const { audioExport } = session.actions;
  const [path, setPath] = createSignal("");
  const [format, setFormat] = createSignal<ExportFormat>("wav");
  const [sampleRate, setSampleRate] = createSignal(DEFAULT_SAMPLE_RATE);
  const [bitDepth, setBitDepth] = createSignal<BitDepth>("int24");
  const [bitrate, setBitrate] = createSignal<Mp3Bitrate>(320);
  /** Dither; `null` means the backend default: on for 16-bit, off for 24-bit. */
  const [dither, setDither] = createSignal<boolean | null>(null);
  const [loudness, setLoudness] = createSignal<LoudnessChoice>("off");
  const [customLufs, setCustomLufs] = createSignal(-14);
  const [ceiling, setCeiling] = createSignal(DEFAULT_CEILING_DB);
  const [range, setRange] = createSignal<RangeChoice>("song");
  const [stems, setStems] = createSignal(false);
  const [tail, setTail] = createSignal(DEFAULT_TAIL_SECONDS);
  const [busy, setBusy] = createSignal(false);
  void audioExport.suggestedPath(format()).then((suggested) => !path() && setPath(suggested));

  const integer = () => isIntegerPcm(format(), bitDepth());
  const ditherOn = () => dither() ?? defaultDither(bitDepth());
  const selection = () => clipsRange(session.project.clips, session.selection.clips());

  const chooseFormat = (next: ExportFormat) => {
    setFormat(next);
    setPath(pathForFormat(path(), next));
    setBitDepth(fitBitDepth(next, bitDepth()));
  };

  const browse = async () => {
    const chosen = await audioExport.choosePath(format(), path());
    if (chosen) setPath(chosen);
  };

  const start = async () => {
    setBusy(true);
    const result = await audioExport.render(exportOptions({
      path: path(), format: format(), sampleRate: sampleRate(), bitDepth: bitDepth(), bitrate: bitrate(), dither: dither(),
      loudness: loudness(), customLufs: customLufs(), ceilingDb: ceiling(), range: range(), stems: stems(), tailSeconds: tail(),
    }, selection()));
    setBusy(false);
    if (result) props.onClose();
  };

  const msg = () => t().project.exportDialog;
  const row = "grid grid-cols-[130px_1fr] items-center gap-3";
  return (
    <Modal
      title={msg().title}
      width="w-[500px]"
      onClose={() => !busy() && props.onClose()}
      footer={
        <>
          <button class="btn" disabled={busy()} onClick={props.onClose}>{t().common.cancel}</button>
          <button class="btn btn-primary" disabled={busy() || !path().trim()} onClick={() => void start()}>{busy() ? msg().rendering : msg().export}</button>
        </>
      }
    >
      <div class="flex flex-col gap-3">
        <div class={row}><span class="label">{msg().file(format())}</span><div class="flex gap-2"><input class="field min-w-0 flex-1 font-mono" value={path()} onInput={(e) => setPath(e.currentTarget.value)} spellcheck={false} aria-label={msg().filePath} /><button class="btn" onClick={() => void browse()}>{msg().browse}</button></div></div>
        <label class={row}><span class="label">{msg().format}</span>
          <select class="field" value={format()} onChange={(e) => chooseFormat(e.currentTarget.value as ExportFormat)}>
            <For each={EXPORT_FORMATS}>{(f) => <option value={f}>{msg().formats[f]}</option>}</For>
          </select></label>
        <label class={row}><span class="label">{msg().sampleRate}</span>
          <select class="field" value={sampleRate()} onChange={(e) => setSampleRate(Number(e.currentTarget.value))}>
            <For each={SAMPLE_RATES}>{(rate) => <option value={rate}>{msg().hz(rate)}</option>}</For>
          </select></label>
        <Show
          when={format() !== "mp3"}
          fallback={
            <label class={row}><span class="label">{msg().bitrate}</span>
              <select class="field" value={bitrate()} onChange={(e) => setBitrate(Number(e.currentTarget.value) as Mp3Bitrate)}>
                <For each={MP3_BITRATES}>{(kbps) => <option value={kbps}>{msg().kbps(kbps)}</option>}</For>
              </select></label>
          }
        >
          <label class={row}><span class="label">{msg().bitDepth}</span>
            <select class="field" value={bitDepth()} onChange={(e) => { setBitDepth(e.currentTarget.value as BitDepth); setDither(null); }}>
              <For each={bitDepthsFor(format())}>{(depth) => <option value={depth}>{msg().bitDepths[depth]}</option>}</For>
            </select></label>
        </Show>
        <label class={row}><span class="label">{msg().normalize}</span>
          <div class="flex gap-2">
            <select class="field min-w-0 flex-1" value={loudness()} onChange={(e) => setLoudness(e.currentTarget.value as LoudnessChoice)}>
              <For each={LOUDNESS_CHOICES}>{(l) => <option value={l.value}>{msg().loudness[l.value]}</option>}</For>
            </select>
            <Show when={loudness() === "custom"}>
              <input class="field w-20" type="number" min="-40" max="0" step="0.5" value={customLufs()} onInput={(e) => setCustomLufs(Number(e.currentTarget.value))} aria-label={msg().customLufs} />
            </Show>
          </div></label>
        <Show when={loudness() !== "off"}>
          <label class={row}><span class="label">{msg().ceiling}</span><input class="field" type="number" min="-12" max="0" step="0.1" value={ceiling()} onInput={(e) => setCeiling(Number(e.currentTarget.value))} /></label>
        </Show>
        <label class={row}><span class="label">{msg().range}</span>
          <select class="field" value={range()} onChange={(e) => setRange(e.currentTarget.value as RangeChoice)}>
            <option value="song">{msg().ranges.song}</option>
            <option value="loop" disabled={!session.project.loopRegion}>{msg().ranges.loop}</option>
            <option value="selection" disabled={!selection()}>{msg().ranges.selection}</option>
          </select></label>
        <label class={row}><span class="label">{msg().tail}</span><input class="field" type="number" min="0" max={MAX_TAIL_SECONDS} step="0.5" value={tail()} onInput={(e) => setTail(Number(e.currentTarget.value))} /></label>
        <Show when={integer()}>
          <label class="flex items-center gap-2 text-sm"><input type="checkbox" checked={ditherOn()} onChange={(e) => setDither(e.currentTarget.checked)} /> {msg().dither}</label>
        </Show>
        <label class="flex items-center gap-2 text-sm"><input type="checkbox" checked={stems()} onChange={(e) => setStems(e.currentTarget.checked)} /> {msg().stems}</label>
        <p class="text-xs text-muted">
          {msg().tailNote}
          <Show when={loudness() !== "off"}>{msg().loudnessNote}</Show>
        </p>
      </div>
    </Modal>
  );
}
